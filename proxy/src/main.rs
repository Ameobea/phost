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
#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate regex;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::{self, Future};
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};
use regex::Regex;

pub mod conf;
pub mod models;
pub mod schema;
use crate::conf::CONF;

lazy_static! {
    static ref REQUEST_URL_RGX: Regex = Regex::new("/([^/]+)/(.*)").unwrap();
}

fn build_conn_pool() -> diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::MysqlConnection>>
{
    let manager = diesel::r2d2::ConnectionManager::new(&CONF.database_url);
    diesel::r2d2::Builder::new()
        .build(manager)
        .expect("Failed to build R2D2/Diesel MySQL Connection Pool")
}

type BoxFut = Box<dyn Future<Item = Response<Body>, Error = hyper::Error> + Send>;

fn debug_request(req: Request<Body>) -> BoxFut {
    let body_str = format!("{:?}", req);
    let response = Response::new(Body::from(body_str));
    Box::new(future::ok(response))
}

fn early_return(status_code: u16, msg: String) -> BoxFut {
    let mut res = Response::new(Body::from(msg));
    *res.status_mut() = hyper::StatusCode::from_u16(status_code).unwrap();
    return box future::ok(res);
}

fn main() {
    dotenv::dotenv().expect("dotenv file parsing failed");

    let pool = Arc::new(Mutex::new(build_conn_pool()));
    // TODO: Populate these from the database
    let proxy_deployments: Arc<Mutex<HashMap<String, String>>> =
        Arc::new(Mutex::new(HashMap::new()));

    {
        proxy_deployments
            .lock()
            .unwrap()
            .insert("test".into(), "http://the4chandiscord.xyz".into())
    };

    let addr = ([0, 0, 0, 0], CONF.port).into();

    // A `Service` is needed for every connection.
    let make_svc = make_service_fn(move |socket: &AddrStream| {
        let remote_addr = socket.remote_addr();
        let proxy_deployments = Arc::clone(&proxy_deployments);

        // TODO: Expose endpoints for getting/setting/updating in-memory proxy mappings

        service_fn(move |req: Request<Body>| {
            let path = req.uri().path();
            let (subdomain, path): (String, String) = match REQUEST_URL_RGX.captures(path) {
                Some(caps) => (String::from(&caps[1]), String::from(&caps[2])),
                None => {
                    return early_return(400, "Invalid URL; format is /subdomain/[...path]".into());
                }
            };

            match { &(*proxy_deployments).lock().unwrap().get(&subdomain).clone() } {
                Some(dst_url) => {
                    let joined_path = format!("{}/{}", dst_url, path);
                    hyper_reverse_proxy::call(remote_addr.ip(), &joined_path, req)
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
