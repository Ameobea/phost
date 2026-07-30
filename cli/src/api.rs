use std::collections::BTreeMap;

use anyhow::{bail, Result};
use reqwest::blocking::multipart::{Form, Part};
use reqwest::blocking::Client;
use reqwest::Method;
use serde_json::Value;

use crate::config::{load_cookies, save_cookies, Conf};

pub enum Body {
    None,
    Form(Vec<(&'static str, String)>),
    Multipart { fields: Vec<(&'static str, String)>, file: Option<Vec<u8>> },
}

pub struct Api {
    client: Client,
    pub conf: Conf,
    cookies: BTreeMap<String, String>,
}

impl Api {
    pub fn new(conf: Conf) -> Api {
        Api { client: Client::new(), conf, cookies: load_cookies() }
    }

    pub fn call(&mut self, method: Method, path: &str, body: &Body) -> Result<Value> {
        for attempt in 0.. {
            let mut req = self
                .client
                .request(method.clone(), format!("{}/{}", self.conf.api_server_url, path));

            if !self.cookies.is_empty() {
                let cookie_str = self
                    .cookies
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                req = req.header("Cookie", cookie_str);
            }

            req = match body {
                Body::None => req,
                Body::Form(pairs) => req.form(&pairs.iter().cloned().collect::<Vec<_>>()),
                Body::Multipart { fields, file } => {
                    let mut form = Form::new();
                    for (k, v) in fields {
                        form = form.text(*k, v.clone());
                    }
                    if let Some(bytes) = file {
                        form = form.part("file", Part::bytes(bytes.clone()).file_name("directory.tgz"));
                    }
                    req.multipart(form)
                }
            };

            let res = match req.send() {
                Ok(res) => res,
                Err(_) => bail!("Error while communicating with the server's API"),
            };

            for sc in res.headers().get_all("set-cookie") {
                if let Some((k, v)) = sc.to_str().ok().and_then(|s| {
                    s.split(';').next().and_then(|kv| kv.split_once('='))
                }) {
                    self.cookies.insert(k.trim().to_string(), v.to_string());
                }
            }

            match res.status().as_u16() {
                404 => bail!("Resource not found"),
                500 => bail!("Internal server error"),
                403 => {
                    if path != "login/" && attempt == 0 {
                        self.login()?;
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        continue;
                    }
                    bail!("Error logging in; invalid username/password?");
                }
                200 => return Ok(res.json()?),
                code => bail!(
                    "Received {} response code when making request: {}",
                    code,
                    res.text().unwrap_or_default()
                ),
            }
        }
        unreachable!()
    }

    fn login(&mut self) -> Result<()> {
        let body = Body::Form(vec![
            ("username", self.conf.username.clone()),
            ("password", self.conf.password.clone()),
        ]);
        let res = self.call(Method::POST, "login/", &body)?;

        if res["success"].as_bool() != Some(true) {
            bail!("Error logging into the server; invalid username/password?");
        }

        save_cookies(&self.cookies);
        Ok(())
    }
}
