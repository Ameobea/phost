#![feature(proc_macro_hygiene, decl_macro)]

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
extern crate regex;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate signal_hook;

use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
};

use diesel::{
    prelude::*,
    r2d2::{Builder, ConnectionManager, Pool},
    MysqlConnection,
};
use futures::future::{self, Future};
use hyper::{
    http::{
        uri::{PathAndQuery, Uri},
        HeaderMap,
    },
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use regex::Regex;
use signal_hook::{iterator::Signals, SIGUSR1};

pub mod conf;
pub mod models;
pub mod schema;
use crate::{conf::CONF, models::ProxyDeployment};

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
    proxy_deployments: &Mutex<HashMap<String, ProxyDeployment>>,
    pool: &Mutex<Pool<ConnectionManager<MysqlConnection>>>,
) {
    use crate::schema::serversite_proxydeployment::dsl::*;

    let active_deployments = {
        let pool_inner = &mut *pool.lock().unwrap();
        let conn = pool_inner.get().expect("Failed to get connection from connection pool");

        match serversite_proxydeployment
            .limit(1_000_000_000)
            .load::<ProxyDeployment>(&conn)
        {
            Ok(res) => res,
            Err(err) => {
                error!("Error loading proxy deployments from the database: {:?}", err);
                return;
            },
        }
    };
    info!(
        "Retrieved {} deployments; updating active deployments map...",
        active_deployments.len()
    );

    let deployments_inner = &mut *proxy_deployments.lock().unwrap();
    deployments_inner.clear();

    for deployment in active_deployments {
        debug!(
            "Registering proxy {} : {}",
            deployment.subdomain, deployment.destination_address
        );
        deployments_inner.insert(deployment.subdomain.clone(), deployment);
    }
    info!("Active deployments map updated");
}

fn remove_all_headers(headers: &mut HeaderMap, header_name: &str) {
    loop {
        let removed_header = headers.remove(header_name);
        if removed_header.is_none() {
            break;
        }
    }
}

/// Registers signal handlers that trigger the set of active deployments to be reloaded from the
/// database
fn init_signal_handlers(proxy_deployments: Arc<Mutex<HashMap<String, ProxyDeployment>>>) {
    let pool = Arc::new(Mutex::new(build_conn_pool()));

    // Populate the proxy deployments with the initial set of
    populate_proxy_deployments(&*proxy_deployments, &*pool);

    let signals = Signals::new(&[SIGUSR1]).expect("Failed to create `signals` handle");
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
    return Box::new(future::ok(res));
}

fn setup_logger() {
    fern::Dispatch::new()
        .level(log::LevelFilter::Debug)
        .level(log::LevelFilter::Debug)
        .level_for("hyper", log::LevelFilter::Info)
        .level_for("mio", log::LevelFilter::Info)
        .level_for("tokio_core", log::LevelFilter::Info)
        .level_for("tokio_reactor", log::LevelFilter::Info)
        .chain(fern::log_file("/dev/stdout").expect("Error chaining Fern output to /dev/stdout"))
        .apply()
        .expect("Failed to apply Fern dispatch");
}

fn main() {
    setup_logger();

    let proxy_deployments: Arc<Mutex<HashMap<String, ProxyDeployment>>> = Arc::new(Mutex::new(HashMap::new()));

    init_signal_handlers(Arc::clone(&proxy_deployments));

    let addr = ([0, 0, 0, 0], CONF.port).into();

    // A `Service` is needed for every connection.
    let make_svc = make_service_fn(move |socket: &AddrStream| {
        let remote_addr = socket.remote_addr();
        let proxy_deployments = Arc::clone(&proxy_deployments);

        service_fn(move |mut req: Request<Body>| {
            let req_path_and_query = req.uri().path_and_query().map(|pnq| pnq.as_str()).unwrap_or_else(|| "");
            let (subdomain, path): (String, String) = match REQUEST_URL_RGX.captures(req_path_and_query) {
                Some(caps) => (String::from(&caps[1]), String::from(&caps[2])),
                None => return early_return(400, "Invalid URL; format is /subdomain/[...path]".into()),
            };

            let deployment_descriptor = match {
                proxy_deployments
                    .lock()
                    .unwrap()
                    .get(&subdomain)
                    .map(|deployment_descriptor| deployment_descriptor.clone())
            } {
                Some(deployment_descriptor) => deployment_descriptor,
                None => return early_return(404, format!("Deployment \"{}\" not found.", subdomain)),
            };

            let dst_url = deployment_descriptor.destination_address.clone();
            let mut uri_parts = req.uri().clone().into_parts();
            let dst_pnq = match PathAndQuery::from_str(&format!("/{}", path)) {
                Ok(pnq) => pnq,
                Err(_) => return early_return(500, "Failed to build `PathAndQuery` from path+query string".into()),
            };
            uri_parts.path_and_query = Some(dst_pnq);

            let uri = match Uri::from_parts(uri_parts) {
                Ok(uri) => uri,
                Err(_) => return early_return(500, "Unable to convert URI parts to URI".into()),
            };
            *req.uri_mut() = uri;

            // Set the `Host` header to be accurate for the destination
            let dst_uri = match Uri::from_str(&dst_url) {
                Ok(uri) => uri,
                Err(_) =>
                    return early_return(
                        400,
                        format!("Invalid target URL provided for this proxy: \"{}\"", dst_url),
                    ),
            };
            let dst_uri_parts = dst_uri.into_parts();
            let authority = dst_uri_parts.authority.unwrap();
            let host = authority.host();

            remove_all_headers(req.headers_mut(), "HOST");
            req.headers_mut().insert("HOST", host.parse().unwrap());

            let use_cors_headers = deployment_descriptor.use_cors_headers;
            info!("[REQ] {}/{}", dst_url, path);
            Box::new(
                hyper_reverse_proxy::call(remote_addr.ip(), &dst_url, req).and_then(move |mut res| {
                    // Set CORS headers if that option is enabled
                    if use_cors_headers {
                        debug!("Adding CORS header");
                        remove_all_headers(res.headers_mut(), "Access-Control-Allow-Origin");
                        res.headers_mut()
                            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
                    }

                    info!("[RES] {}/{}", dst_url, path);
                    Ok(res)
                }),
            )
        })
    });

    let server = Server::bind(&addr)
        .serve(make_svc)
        .map_err(|e| eprintln!("server error: {}", e));

    println!("Running server on {:?}", addr);

    // Run this server for... forever!
    hyper::rt::run(server);
}
