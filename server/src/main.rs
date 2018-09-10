#![allow(proc_macro_derive_resolution_fallback)]

extern crate actix;
extern crate actix_web;
#[macro_use]
extern crate diesel;
extern crate dotenv;

pub mod db;
pub mod models;
pub mod schema;

fn main() {
    println!("Hello, world!");
}
