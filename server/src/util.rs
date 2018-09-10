use config::CONF;

pub fn get_deployment_url(subdomain: &str) -> String {
    format!(
        "http{}://{}.{}/",
        if CONF.https { "s" } else { "" },
        subdomain,
        CONF.deployment_url_root
    )
}
