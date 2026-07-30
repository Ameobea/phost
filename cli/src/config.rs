use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Conf {
    pub api_server_url: String,
    pub hosting_base_url: String,
    pub hosting_protocol: String,
    pub username: String,
    pub password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<DockerConf>,
}

#[derive(Serialize, Deserialize)]
pub struct DockerConf {
    pub builder: Option<String>,
    pub target: Option<String>,
    #[serde(default = "default_true")]
    pub forward_agent: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Conf {
    fn default() -> Self {
        Conf {
            api_server_url: "http://localhost:8000".into(),
            hosting_base_url: "ameo.design".into(),
            hosting_protocol: "https".into(),
            username: "user".into(),
            password: "pass".into(),
            docker: None,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs_home().join(".phost")
}

pub fn default_conf_path() -> PathBuf {
    config_dir().join("conf.toml")
}

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME not set"))
}

pub fn load_conf(path: &Path) -> Result<Conf> {
    if !path.is_file() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string(&Conf::default())?)?;
        bail!(
            "Created default config file: {}; you must fill in the values yourself.",
            path.display()
        );
    }

    toml::from_str(&std::fs::read_to_string(path)?).context("Error reading supplied config file!")
}

pub fn load_cookies() -> BTreeMap<String, String> {
    std::fs::read_to_string(config_dir().join("cookies.toml"))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_cookies(cookies: &BTreeMap<String, String>) {
    if let Ok(s) = toml::to_string(cookies) {
        let _ = std::fs::write(config_dir().join("cookies.toml"), s);
    }
}
