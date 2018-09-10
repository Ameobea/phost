#![feature(box_syntax)]
#![allow(proc_macro_derive_resolution_fallback)]

extern crate actix;
extern crate actix_web;
#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate futures;
extern crate r2d2;
extern crate structopt;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;
extern crate tempfile;
extern crate tokio_fs;
extern crate tokio_process;
extern crate uuid;

use std::env;

use actix::{Addr, SyncArbiter};
use actix_web::{http, middleware, server, App};
use diesel::r2d2::ConnectionManager;
use diesel::MysqlConnection;
use structopt::StructOpt;

pub mod config;
pub mod db;
pub mod models;
pub mod routes;
pub mod schema;
pub mod upload;
pub mod util;

use config::CONF;
use db::DbExecutor;
use routes::{create_static_deployment, index, list_deployments};

/// State with DbExecutor address
pub struct AppState {
    db: Addr<DbExecutor>,
}

fn main() {
    let sys = actix::System::new("diesel-example");
    let manager: ConnectionManager<MysqlConnection> = ConnectionManager::new(
        env::var("DATABASE_URL").expect("Must supply `DATABASE_URL` environment variable!"),
    );
    println!("Creating MySQL connection pool...");
    let pool = r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create MySQL connection pool.");
    println!("MySQL connection pool successfully created.");

    let addr = SyncArbiter::start(CONF.worker_threads, move || DbExecutor(pool.clone()));

    println!("Initializing webserver...");
    server::new(move || {
        App::with_state(AppState { db: addr.clone() })
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(http::Method::GET).with(index))
            .resource("/deployments", |r| {
                r.method(http::Method::GET).with_async(list_deployments)
            }).resource("/create_static_deployment", |r| {
                r.method(http::Method::POST)
                    .with_async(create_static_deployment)
            })
    }).bind(&format!("{}:{}", CONF.bind, CONF.port))
    .unwrap()
    .start();

    println!("Webserver initialized; starting webserver...");
    let _ = sys.run();
}
