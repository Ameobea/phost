use std::env;

pub struct Conf {
    pub database_url: String,
    pub port: u16,
}

impl Conf {
    pub fn build_from_env() -> Self {
        dotenv::dotenv().expect("dotenv file parsing failed");

        Conf {
            database_url: env::var("DATABASE_URL").expect("The `DATABASE_URL` environment variable must be supplied"),
            port: env::var("PORT")
                .unwrap_or_else(|_| -> String { "5855".into() })
                .parse()
                .expect("Unable to convert `PORT` to `u16`"),
        }
    }
}

lazy_static! {
    pub static ref CONF: Conf = Conf::build_from_env();
}
