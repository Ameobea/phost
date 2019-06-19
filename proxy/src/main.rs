#![feature(
    box_syntax,
    proc_macro_hygiene,
    decl_macro,
    slice_patterns,
    bind_by_move_pattern_guards,
    box_patterns,
    nll,
    try_trait
)]

extern crate chrono;
extern crate futures;
extern crate hyper;
extern crate hyper_reverse_proxy;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate fern;
#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate regex;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate signal_hook;

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use diesel::prelude::*;
use diesel::r2d2::{Builder, ConnectionManager, Pool};
use diesel::MysqlConnection;
use futures::future::{self, Future};
use hyper::http::uri::{PathAndQuery, Uri};
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use regex::Regex;
use signal_hook::{iterator::Signals, SIGUSR1};

pub mod conf;
pub mod models;
pub mod schema;
use crate::conf::CONF;

lazy_static! {
    static ref REQUEST_URL_RGX: Regex = Regex::new("/([^/]+)/(.*)").unwrap();
}

fn build_conn_pool() -> Pool<ConnectionManager<MysqlConnection>> {
    let manager = ConnectionManager::new(&CONF.database_url);
    Builder::new()
        .build(manager)
        .expect("Failed to build R2D2/Diesel MySQL Connection Pool")
}

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

/// Loads the full set of deployments from the database and updates the mapping with them.
fn populate_proxy_deployments(
    proxy_deployments: &Mutex<HashMap<String, String>>,
    pool: &Mutex<Pool<ConnectionManager<MysqlConnection>>>,
) {
    use crate::schema::serversite_proxydeployment::dsl::*;

    let active_deployments = {
        let pool_inner = &mut *pool.lock().unwrap();
        let conn = pool_inner
            .get()
            .expect("Failed to get connection from connection pool");

        match serversite_proxydeployment
            .limit(1_000_000_000)
            .load::<crate::models::ProxyDeployment>(&conn)
        {
            Ok(res) => res,
            Err(err) => {
                error!(
                    "Error loading proxy deployments from the database: {:?}",
                    err
                );
                return;
            }
        }
    };
    info!("Retrieved {} deployments; updating active deployments map...", active_deployments.len());

    let deployments_inner = &mut *proxy_deployments.lock().unwrap();
    deployments_inner.clear();

    for deployment in active_deployments {
        deployments_inner.insert(deployment.subdomain, deployment.destination_address);
    }
    info!("Active deployments map updated");
}

fn init_signal_handlers(proxy_deployments: Arc<Mutex<HashMap<String, String>>>) {
    let pool = Arc::new(Mutex::new(build_conn_pool()));

    // Populate the proxy deployments with the initial set of
    populate_proxy_deployments(&*proxy_deployments, &*pool);

    let signals = Signals::new(&[signal_hook::SIGUSR1]).expect("Failed to create `signals` handle");
    std::thread::spawn(move || {
        for _signal in signals.forever() {
            info!("Received signal; updating deployments from database...");
            populate_proxy_deployments(&*proxy_deployments, &*pool);
            info!("Finished updating deployments.")
        }
    });
}

fn early_return(status_code: u16, msg: String) -> BoxFut {
    let mut res = Response::new(Body::from(msg));
    *res.status_mut() = hyper::StatusCode::from_u16(status_code).unwrap();
    return box future::ok(res);
}

fn setup_logger() {
    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout())
        .apply()
        .expect("Failed to apply Ferm dispatch");
}

fn main() {
    dotenv::dotenv().expect("dotenv file parsing failed");

    setup_logger();

    let proxy_deployments: Arc<Mutex<HashMap<String, String>>> =
        Arc::new(Mutex::new(HashMap::new()));

    init_signal_handlers(Arc::clone(&proxy_deployments));

    let addr = ([0, 0, 0, 0], CONF.port).into();

    // A `Service` is needed for every connection.
    let make_svc = make_service_fn(move |socket: &AddrStream| {
        let remote_addr = socket.remote_addr();
        let proxy_deployments = Arc::clone(&proxy_deployments);

        service_fn(move |mut req: Request<Body>| {
            let req_path_and_query = req
                .uri()
                .path_and_query()
                .map(|pnq| pnq.as_str())
                .unwrap_or_else(|| "");
            let (subdomain, path): (String, String) = match REQUEST_URL_RGX
                .captures(req_path_and_query)
            {
                Some(caps) => (String::from(&caps[1]), String::from(&caps[2])),
                None => {
                    return early_return(400, "Invalid URL; format is /subdomain/[...path]".into());
                }
            };

            match { &(*proxy_deployments).lock().unwrap().get(&subdomain).clone() } {
                Some(dst_url) => {
                    let mut uri_parts = req.uri().clone().into_parts();
                    let dst_pnq = match PathAndQuery::from_str(&format!("/{}", path)) {
                        Ok(pnq) => pnq,
                        Err(_) => {
                            return early_return(
                                500,
                                "Failed to build `PathAndQuery` from path+query string".into(),
                            );
                        }
                    };
                    uri_parts.path_and_query = Some(dst_pnq);

                    let uri = match Uri::from_parts(uri_parts) {
                        Ok(uri) => uri,
                        Err(_) => {
                            return early_return(500, "Unable to convert URI parts to URI".into());
                        }
                    };
                    *req.uri_mut() = uri;

                    // Set the `Host` header to be accurate for the destination
                    let uri_parts = Uri::from_str(dst_url).unwrap().into_parts();
                    let authority = uri_parts.authority.unwrap();
                    let host = authority.host();
                    loop {
                        let removed_header = req.headers_mut().remove("HOST");
                        if removed_header.is_none() {
                            break;
                        }
                    }
                    req.headers_mut().insert("HOST", host.parse().unwrap());

                    hyper_reverse_proxy::call(remote_addr.ip(), &dst_url, req)
                }
                None => early_return(404, format!("Deployment \"{}\" not found.", subdomain)),
            }
        })
    });

    let server = Server::bind(&addr)
        .serve(make_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Running server on {:?}", addr);

    // Run this server for... forever!
    hyper::rt::run(server);
}
