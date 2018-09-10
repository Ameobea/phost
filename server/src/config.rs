//! Defines the configuration for the application parsed from environment variables and command
//! line arguments at runtime.

use std::path::PathBuf;

use structopt::StructOpt;

lazy_static! {
    pub static ref CONF: Config = Config::from_args();
}

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
pub struct Config {
    #[structopt(short = "b", long = "bind", default_value = "127.0.0.1")]
    pub bind: String,

    #[structopt(short = "p", long = "port", default_value = "8448")]
    pub port: u16,

    #[structopt(short = "s", long = "https")]
    pub https: bool,

    #[structopt(
        short = "u",
        long = "deployment-url-root",
        default_value = "localhost"
    )]
    pub deployment_url_root: String,

    #[structopt(short = "w", long = "workers", default_value = "8")]
    pub worker_threads: usize,

    #[structopt(
        short = "f",
        long = "hosting-dir",
        default_value = "./hosting",
        parse(from_os_str)
    )]
    pub hosting_dir: PathBuf,
}
