mod api;
mod config;
mod deploy;
mod util;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use reqwest::Method;
use serde_json::Value;

use api::{Api, Body};
use util::{compress_dir, create_random_subdomain, print_table, slugify};

const UPDATE_HELP: &str = "VERSION: The new version to deploy.  This takes some special values:
 - patch/p: Increments the patch version of the current version
 - minor/m: Increments the minor version of the current version
 - major/M: Increments the major version of the current version";

#[derive(Parser)]
#[command(name = "phost", version)]
struct Cli {
    #[arg(short = 'c', long = "config")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct Lookup {
    query: String,
    /// Look up deployment by name (default)
    #[arg(long)]
    name: bool,
    /// Look up deployment by ID
    #[arg(long)]
    id: bool,
    /// Look up deployment by subdomain
    #[arg(long)]
    subdomain: bool,
}

impl Lookup {
    fn field(&self) -> &'static str {
        if self.id {
            "id"
        } else if self.subdomain {
            "subdomain"
        } else {
            "name"
        }
    }
}

#[derive(Args)]
struct RmArgs {
    #[command(flatten)]
    lookup: Lookup,
    /// If supplied, only this version will be deleted.  If not supplied, all versions will be
    /// deleted.
    #[arg(short = 'v', long)]
    version: Option<String>,
}

#[derive(Args)]
struct CreateArgs {
    name: String,
    directory: String,
    /// The subdomain on which the deployment will be hosted.  If left off, the subdomain will be
    /// constructed from the deployment name.
    #[arg(short = 's', long)]
    subdomain: Option<String>,
    #[arg(short = 'v', long, default_value = "0.1.0")]
    version: String,
    /// Private deployments have a randomized subdomain
    #[arg(short = 'p', long)]
    private: bool,
    /// Create this deployment as a single-page web application where `index.html` is served for
    /// all missing routes
    #[arg(long)]
    spa: bool,
    /// A path to a file that will be served in case of a 404.  If not provided, the default
    /// Apache2 "Not Found" page will be displayed.
    #[arg(long)]
    not_found_document: Option<String>,
    /// A string representing a category that this deployment should be added to.  (Multiple may
    /// be provided)
    #[arg(short = 'c', long = "category")]
    categories: Vec<String>,
}

#[derive(Args)]
#[command(after_help = UPDATE_HELP)]
struct UpdateArgs {
    #[command(flatten)]
    lookup: Lookup,
    version: String,
    directory: String,
}

#[derive(Args)]
struct ProxyCreateArgs {
    name: String,
    target_url: String,
    /// The subdomain on which the proxy will be mounted.  If left off, the subdomain will be
    /// constructed from the proxy name.
    #[arg(short = 's', long)]
    subdomain: Option<String>,
    /// Use a randomized subdomain
    #[arg(short = 'p', long)]
    private: bool,
    /// Enable CORS headers on the proxy
    #[arg(short = 'c', long)]
    cors: bool,
}

#[derive(Args)]
struct DeployArgs {
    /// Push the current branch to its upstream before deploying
    #[arg(long)]
    push: bool,
}

#[derive(Args)]
struct LogsArgs {
    /// Follow log output
    #[arg(short = 'f', long)]
    follow: bool,
    /// Number of trailing lines to show
    #[arg(short = 'n', long, default_value_t = 100)]
    tail: u32,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build and deploy the Docker project configured in .phost.toml
    Deploy(DeployArgs),
    /// Swap the deployed container back to the previous retained image
    Rollback,
    /// Show deployed container state for the current project
    Status,
    /// Show deployed container logs for the current project
    Logs(LogsArgs),
    /// Shorthand for `phost deployment ls`
    Ls,
    /// Shorthand for `phost deployment show`
    Show(Lookup),
    /// Shorthand for `phost deployment rm`
    Rm(RmArgs),
    /// Shorthand for `phost deployment create`
    Create(CreateArgs),
    /// Shorthand for `phost deployment update`
    Update(UpdateArgs),
    /// Manage static subdomain deployments
    #[command(subcommand)]
    Deployment(DeploymentCmd),
    /// Manage HTTP proxies
    #[command(subcommand)]
    Proxy(ProxyCmd),
}

#[derive(Subcommand)]
enum DeploymentCmd {
    Ls,
    Create(CreateArgs),
    #[command(after_help = UPDATE_HELP)]
    Update(UpdateArgs),
    Rm(RmArgs),
    Show(Lookup),
}

#[derive(Subcommand)]
enum ProxyCmd {
    Ls,
    Create(ProxyCreateArgs),
    Rm(Lookup),
}

fn resolve_subdomain(explicit: Option<String>, private: bool, name: &str) -> Result<String> {
    match (explicit, private) {
        (Some(_), true) => {
            anyhow::bail!("Can't supply both `--random-subdomain` and an explicit subdomain")
        }
        (None, true) => Ok(create_random_subdomain()),
        (Some(s), false) => Ok(s),
        (None, false) => Ok(slugify(name)),
    }
}

fn deployment_url(api: &Api, subdomain: &str, proxy: bool) -> String {
    format!(
        "{}://{}.{}{}/",
        api.conf.hosting_protocol,
        subdomain,
        if proxy { "p." } else { "" },
        api.conf.hosting_base_url
    )
}

fn date_part(v: &Value) -> String {
    v.as_str().unwrap_or("").chars().take(10).collect()
}

fn list_deployments(api: &mut Api) -> Result<()> {
    let deployments = api.call(Method::GET, "deployments/", &Body::None)?;

    let mut rows = vec![
        ["Name", "URL", "Creation Date", "Active Version", "All Versions", "Categories"]
            .map(String::from)
            .to_vec(),
    ];

    for d in deployments.as_array().cloned().unwrap_or_default() {
        let mut versions = d["versions"].as_array().cloned().unwrap_or_default();
        versions.sort_by_key(|v| v["created_on"].as_str().unwrap_or("").to_string());

        let active_version = versions
            .iter()
            .find(|v| v["active"].as_bool() == Some(true))
            .and_then(|v| v["version"].as_str())
            .unwrap_or("None")
            .to_string();
        let all_versions = versions
            .iter()
            .filter_map(|v| v["version"].as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let categories = d["categories"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|c| c["category"].as_str())
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join(", ");

        rows.push(vec![
            d["name"].as_str().unwrap_or("").to_string(),
            deployment_url(api, d["subdomain"].as_str().unwrap_or(""), false),
            date_part(&d["created_on"]),
            active_version,
            all_versions,
            categories,
        ]);
    }

    print_table(&rows);
    Ok(())
}

fn show_deployment(api: &mut Api, lookup: &Lookup) -> Result<()> {
    let path = format!("deployments/{}/?lookupField={}", lookup.query, lookup.field());
    let data = api.call(Method::GET, &path, &Body::None)?;

    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    serde::Serialize::serialize(&data, &mut ser)?;
    println!("{}", String::from_utf8(buf)?);
    Ok(())
}

fn delete_deployment(api: &mut Api, args: &RmArgs) -> Result<()> {
    let path = match &args.version {
        None => format!("deployments/{}/?lookupField={}", args.lookup.query, args.lookup.field()),
        Some(v) => format!(
            "deployments/{}/{}/?lookupField={}",
            args.lookup.query,
            v,
            args.lookup.field()
        ),
    };
    api.call(Method::DELETE, &path, &Body::None)?;

    println!(
        "Deployment {}successfully deleted",
        if args.version.is_none() { "" } else { "version " }
    );
    Ok(())
}

fn create_deployment(api: &mut Api, args: CreateArgs) -> Result<()> {
    let not_found_document = match (args.spa, args.not_found_document) {
        (true, None) => Some("./index.html".to_string()),
        (_, nfd) => nfd,
    };
    let subdomain = resolve_subdomain(args.subdomain, args.private, &args.name)?;

    let tgz = compress_dir(&args.directory)?;
    let mut fields = vec![
        ("name", args.name),
        ("subdomain", subdomain),
        ("version", args.version),
        ("categories", args.categories.join(",")),
    ];
    if let Some(nfd) = not_found_document {
        fields.push(("not_found_document", nfd));
    }

    let res = api.call(
        Method::POST,
        "deployments/",
        &Body::Multipart { fields, file: Some(tgz) },
    )?;
    println!("Deployment successfully created: {}", res["url"].as_str().unwrap_or(""));
    Ok(())
}

fn update_deployment(api: &mut Api, args: &UpdateArgs) -> Result<()> {
    let tgz = compress_dir(&args.directory)?;
    let path = format!(
        "deployments/{}/{}/?lookupField={}",
        args.lookup.query,
        args.version,
        args.lookup.field()
    );
    api.call(Method::POST, &path, &Body::Multipart { fields: vec![], file: Some(tgz) })?;

    println!("Deployment successfully updated");
    Ok(())
}

fn list_proxies(api: &mut Api) -> Result<()> {
    let proxies = api.call(Method::GET, "proxy/", &Body::None)?;

    let mut rows = vec![["Name", "URL", "Creation Date"].map(String::from).to_vec()];
    for p in proxies.as_array().cloned().unwrap_or_default() {
        rows.push(vec![
            p["name"].as_str().unwrap_or("").to_string(),
            deployment_url(api, p["subdomain"].as_str().unwrap_or(""), true),
            date_part(&p["created_on"]),
        ]);
    }

    print_table(&rows);
    Ok(())
}

fn create_proxy(api: &mut Api, args: ProxyCreateArgs) -> Result<()> {
    let subdomain = resolve_subdomain(args.subdomain, args.private, &args.name)?;

    let fields = vec![
        ("name", args.name),
        ("subdomain", subdomain),
        ("use_cors_headers", if args.cors { "True".into() } else { "False".into() }),
        ("destination_address", args.target_url),
    ];
    let res = api.call(Method::POST, "proxy/", &Body::Multipart { fields, file: None })?;
    println!("Proxy successfully created: {}", res["url"].as_str().unwrap_or(""));
    Ok(())
}

fn delete_proxy(api: &mut Api, lookup: &Lookup) -> Result<()> {
    let path = format!("proxy/{}/?lookupField={}", lookup.query, lookup.field());
    api.call(Method::DELETE, &path, &Body::None)?;

    println!("Proxy successfully deleted");
    Ok(())
}

fn run(cmd: Cmd, conf: config::Conf) -> Result<()> {
    match &cmd {
        Cmd::Deploy(args) => return deploy::deploy(&conf, args.push),
        Cmd::Rollback => return deploy::rollback(&conf),
        Cmd::Status => return deploy::status(&conf),
        Cmd::Logs(args) => return deploy::logs(&conf, args.follow, args.tail),
        _ => {}
    }

    let api = &mut Api::new(conf);
    match cmd {
        Cmd::Deploy(_) | Cmd::Rollback | Cmd::Status | Cmd::Logs(_) => unreachable!(),
        Cmd::Ls | Cmd::Deployment(DeploymentCmd::Ls) => list_deployments(api),
        Cmd::Show(lookup) | Cmd::Deployment(DeploymentCmd::Show(lookup)) => {
            show_deployment(api, &lookup)
        }
        Cmd::Rm(args) | Cmd::Deployment(DeploymentCmd::Rm(args)) => delete_deployment(api, &args),
        Cmd::Create(args) | Cmd::Deployment(DeploymentCmd::Create(args)) => {
            create_deployment(api, args)
        }
        Cmd::Update(args) | Cmd::Deployment(DeploymentCmd::Update(args)) => {
            update_deployment(api, &args)
        }
        Cmd::Proxy(ProxyCmd::Ls) => list_proxies(api),
        Cmd::Proxy(ProxyCmd::Create(args)) => create_proxy(api, args),
        Cmd::Proxy(ProxyCmd::Rm(lookup)) => delete_proxy(api, &lookup),
    }
}

fn main() {
    // Die quietly instead of panicking when stdout is closed early (`phost ls | head`)
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };

    let cli = Cli::parse();

    let result = (|| -> Result<()> {
        let conf = config::load_conf(&cli.config.unwrap_or_else(config::default_conf_path))?;
        run(cli.cmd, conf)
    })();

    if let Err(e) = result {
        println!("Error: {e}");
        std::process::exit(1);
    }
}
