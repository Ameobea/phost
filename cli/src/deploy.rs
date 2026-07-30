use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::config::Conf;

#[derive(Deserialize)]
pub struct ProjectConf {
    pub app: AppConf,
    pub build: BuildConf,
    #[serde(default)]
    pub deploy: DeployConf,
}

#[derive(Deserialize)]
pub struct AppConf {
    pub name: String,
}

#[derive(Deserialize)]
pub struct BuildConf {
    pub commands: Vec<String>,
    pub image: String,
    /// ssh -L specs (`localport:host:port`) opened toward the deploy target during builds
    #[serde(default)]
    pub tunnels: Vec<String>,
}

#[derive(Deserialize, Default)]
pub struct DeployConf {
    pub target: Option<String>,
    pub health: Option<HealthConf>,
    #[serde(default)]
    pub run: RunConf,
}

#[derive(Deserialize)]
pub struct HealthConf {
    pub url: String,
    #[serde(default = "default_health_timeout")]
    pub timeout_secs: u64,
}

fn default_health_timeout() -> u64 {
    30
}

#[derive(Deserialize)]
pub struct RunConf {
    pub net: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub env_file: Option<String>,
    #[serde(default = "default_restart")]
    pub restart: String,
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub cmd: Vec<String>,
}

impl Default for RunConf {
    fn default() -> Self {
        toml::from_str("").unwrap()
    }
}

fn default_restart() -> String {
    "always".into()
}

fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

struct SshHost {
    user_host: String,
    port: Option<u16>,
}

impl SshHost {
    fn parse(s: &str) -> Result<SshHost> {
        match s.rsplit_once(':') {
            Some((uh, port)) => Ok(SshHost {
                user_host: uh.to_string(),
                port: Some(port.parse().context("invalid ssh port")?),
            }),
            None => Ok(SshHost { user_host: s.to_string(), port: None }),
        }
    }

    fn ssh_args(&self, forward_agent: bool) -> Vec<String> {
        let mut args = vec![
            "-o".into(),
            "ControlMaster=auto".into(),
            "-o".into(),
            "ControlPath=~/.ssh/phost-%C".into(),
            "-o".into(),
            "ControlPersist=10m".into(),
        ];
        if let Some(p) = self.port {
            args.extend(["-p".into(), p.to_string()]);
        }
        if forward_agent {
            args.push("-A".into());
        }
        args.push(self.user_host.clone());
        args
    }

    /// [user@]host[:port] form consumed by `docker pussh`
    fn pussh_dest(&self) -> String {
        match self.port {
            Some(p) => format!("{}:{}", self.user_host, p),
            None => self.user_host.clone(),
        }
    }
}

enum Runner {
    Local(PathBuf),
    Remote { host: SshHost, forward_agent: bool },
}

impl Runner {
    /// Runs `script` with stdio inherited so output streams live.
    fn run(&self, script: &str) -> Result<()> {
        let status = match self {
            Runner::Local(dir) => {
                Command::new("bash").arg("-c").arg(script).current_dir(dir).status()?
            }
            Runner::Remote { host, forward_agent } => Command::new("ssh")
                .args(host.ssh_args(*forward_agent))
                .arg("-t")
                .arg("--")
                .arg(format!("exec \"$SHELL\" -ilc {}", shq(script)))
                .status()?,
        };
        if !status.success() {
            bail!("command failed with {}", status);
        }
        Ok(())
    }

    /// Runs a POSIX `script` remotely and captures stdout (no tty, no login shell).
    fn output(&self, script: &str) -> Result<String> {
        let Runner::Remote { host, .. } = self else { unreachable!() };
        let out = Command::new("ssh")
            .args(host.ssh_args(false))
            .arg("--")
            .arg(script)
            .output()?;
        if !out.status.success() {
            bail!("remote command failed: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output()?;
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn find_project_conf() -> Result<(ProjectConf, PathBuf)> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join(".phost.toml");
        if candidate.is_file() {
            let pc = toml::from_str(&std::fs::read_to_string(&candidate)?)
                .context("Error parsing .phost.toml")?;
            return Ok((pc, dir));
        }
        if !dir.pop() {
            bail!("No .phost.toml found in this directory or any parent");
        }
    }
}

struct GitState {
    branch: String,
    sha: String,
}

fn preflight(root: &Path, push: bool) -> Result<GitState> {
    if !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        bail!("tracked files have uncommitted changes; commit or stash first");
    }

    let branch = git(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch == "HEAD" {
        bail!("detached HEAD; check out a branch first");
    }
    let sha = git(root, &["rev-parse", "HEAD"])?;

    git(root, &["fetch", "--quiet"]).context("git fetch failed")?;

    if push {
        let status = Command::new("git").arg("-C").arg(root).arg("push").status()?;
        if !status.success() {
            bail!("git push failed");
        }
    }

    let upstream = git(root, &["rev-parse", "@{u}"])
        .context("no upstream configured for this branch")?;
    if upstream != sha {
        bail!("HEAD is not pushed to upstream; `git push` first or re-run with --push");
    }

    Ok(GitState { branch, sha })
}

fn tunnel_clause(tunnels: &[String], target: &SshHost) -> String {
    if tunnels.is_empty() {
        return String::new();
    }

    let mut s = String::from(
        "TUNNEL_PIDS=\"\"\n\
         cleanup_tunnels() { [ -n \"$TUNNEL_PIDS\" ] && eval \"kill $TUNNEL_PIDS\" 2>/dev/null || true; }\n\
         trap cleanup_tunnels EXIT\n",
    );
    let port_flag = target.port.map(|p| format!(" -p {p}")).unwrap_or_default();
    for spec in tunnels {
        let lport = spec.split(':').next().unwrap();
        s.push_str(&format!(
            r#"if nc -z -w 1 127.0.0.1 {lport} >/dev/null 2>&1; then
  echo '[tunnel] port {lport} already in use; assuming {spec} is available'
else
  echo '[tunnel] opening {spec} via {dest}'
  ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes{port_flag} -L {spec_q} {dest_q} &
  TUNNEL_PIDS="$TUNNEL_PIDS $!"
  I=0
  until nc -z -w 1 127.0.0.1 {lport} >/dev/null 2>&1; do
    I=$((I+1))
    if [ "$I" -ge 20 ]; then echo 'tunnel {spec} failed to open' >&2; exit 1; fi
    sleep 0.5
  done
fi
"#,
            dest = target.user_host,
            spec_q = shq(spec),
            dest_q = shq(&target.user_host),
        ));
    }
    s
}

fn build_script(
    pc: &ProjectConf,
    git_state: &GitState,
    relpath: &str,
    tag: &str,
    remote: bool,
    target: &SshHost,
) -> String {
    let mut script = String::from("set -e\n");
    if remote {
        script.push_str(&format!(
            r#"cd "$HOME"/{relpath}
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then echo 'builder has uncommitted changes to tracked files' >&2; exit 1; fi
git fetch --quiet
git checkout --quiet {branch}
git merge --ff-only --quiet origin/{branch}
if [ "$(git rev-parse HEAD)" != "{sha}" ]; then echo 'builder HEAD does not match pushed sha' >&2; exit 1; fi
"#,
            relpath = shq(relpath),
            branch = shq(&git_state.branch),
            sha = git_state.sha,
        ));
    }
    script.push_str(&tunnel_clause(&pc.build.tunnels, target));
    for cmd in &pc.build.commands {
        script.push_str(cmd);
        script.push('\n');
    }
    script.push_str(&format!("docker tag {} {}\n", shq(&pc.build.image), shq(tag)));
    script
}

fn run_args(pc: &ProjectConf, name: &str, tag: &str) -> String {
    let run = &pc.deploy.run;
    let mut args: Vec<String> = vec![
        "-d".into(),
        "--name".into(),
        shq(name),
        "--restart".into(),
        shq(&run.restart),
    ];
    if let Some(net) = &run.net {
        args.extend(["--net".into(), shq(net)]);
    }
    for p in &run.ports {
        args.extend(["-p".into(), shq(p)]);
    }
    for (k, v) in &run.env {
        args.extend(["-e".into(), shq(&format!("{k}={v}"))]);
    }
    if run.env_file.is_some() {
        args.push(format!("--env-file \"$HOME\"/.phost/env/{name}.env"));
    }
    for v in &run.volumes {
        args.extend(["-v".into(), shq(v)]);
    }
    args.extend(run.extra_args.iter().map(|a| shq(a)));
    args.push(shq(tag));
    args.extend(run.cmd.iter().map(|c| shq(c)));
    args.join(" ")
}

fn swap_script(
    name: &str,
    repo: &str,
    tag: &str,
    run_args: &str,
    health: Option<&HealthConf>,
    rm_failed_image: bool,
) -> String {
    let wait_clause = match health {
        Some(h) => format!(
            r#"DEADLINE=$(( $(date +%s) + {timeout} ))
HEALTHY=""
while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  if [ "$(docker inspect -f '{{{{.RestartCount}}}}' "$NAME" 2>/dev/null || echo 999)" != "0" ]; then break; fi
  if [ "$(docker inspect -f '{{{{.State.Running}}}}' "$NAME" 2>/dev/null)" = "true" ] \
    && curl -sf -o /dev/null --max-time 2 {url}; then HEALTHY=1; break; fi
  sleep 1
done
if [ -z "$HEALTHY" ]; then"#,
            timeout = h.timeout_secs,
            url = shq(&h.url),
        ),
        None => r#"sleep 3
RUNNING=$(docker inspect -f '{{.State.Running}}' "$NAME" 2>/dev/null || echo false)
RESTARTS=$(docker inspect -f '{{.RestartCount}}' "$NAME" 2>/dev/null || echo 999)
if [ "$RUNNING" != "true" ] || [ "$RESTARTS" != "0" ]; then"#
            .to_string(),
    };
    let rm_failed = if rm_failed_image {
        "\n  docker rmi \"$IMG\" >/dev/null 2>&1 || true"
    } else {
        ""
    };

    format!(
        r#"set -e
NAME={name}
IMG={tag}
PREV_IMG=$(docker inspect --format '{{{{.Config.Image}}}}' "$NAME" 2>/dev/null || true)
docker rename "$NAME" "$NAME"-prev 2>/dev/null || true
docker stop "$NAME"-prev >/dev/null 2>&1 || true
docker run {run_args} >/dev/null
{wait_clause}
  echo '--- new container failed health check; logs: ---' >&2
  docker logs --tail 50 "$NAME" 2>&1 || true
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  docker rename "$NAME"-prev "$NAME" 2>/dev/null || true
  docker start "$NAME" >/dev/null 2>&1 || true{rm_failed}
  echo 'deploy failed; previous version restored' >&2
  exit 1
fi
docker rm "$NAME"-prev >/dev/null 2>&1 || true
for img in $(docker images {repo} --format '{{{{.Repository}}}}:{{{{.Tag}}}}'); do
  if [ "$img" != "$IMG" ] && [ "$img" != "$PREV_IMG" ]; then docker rmi "$img" >/dev/null 2>&1 || true; fi
done
PREV_TAG="${{PREV_IMG##*:}}"
CUR_TAG="${{IMG##*:}}"
for ref in $(sudo -n ctr -a /run/containerd/containerd.sock -n moby images ls -q 2>/dev/null | grep "/{repo_raw}:" || true); do
  case "$ref" in
    *:"$CUR_TAG"|*:"$PREV_TAG") ;;
    *) sudo -n ctr -a /run/containerd/containerd.sock -n moby images rm "$ref" >/dev/null 2>&1 || true ;;
  esac
done
"#,
        name = shq(name),
        tag = shq(tag),
        repo = shq(repo),
        repo_raw = repo,
        run_args = run_args,
    )
}

pub fn deploy(conf: &Conf, push: bool) -> Result<()> {
    let (pc, root) = find_project_conf()?;
    let docker_conf = conf
        .docker
        .as_ref()
        .context("no [docker] section in ~/.phost/conf.toml")?;
    let builder = docker_conf
        .builder
        .as_deref()
        .context("no builder configured in [docker] section of ~/.phost/conf.toml")?;
    let target_str = pc
        .deploy
        .target
        .as_deref()
        .or(docker_conf.target.as_deref())
        .context("no deploy target configured (.phost.toml [deploy] or conf.toml [docker])")?;
    let target = SshHost::parse(target_str)?;

    let git_state = preflight(&root, push)?;
    let short_sha = &git_state.sha[..12];
    let name = &pc.app.name;
    let repo = pc.build.image.split(':').next().unwrap().to_string();
    let tag = format!("{repo}:{short_sha}");

    let home = std::env::var("HOME")?;
    let relpath = root
        .strip_prefix(&home)
        .map_err(|_| anyhow::anyhow!("repo must live under $HOME for builder path mapping"))?
        .to_string_lossy()
        .to_string();

    let (build_runner, is_remote) = if builder == "local" {
        (Runner::Local(root.clone()), false)
    } else {
        (
            Runner::Remote {
                host: SshHost::parse(builder)?,
                forward_agent: docker_conf.forward_agent,
            },
            true,
        )
    };

    println!("[build] {name} @ {short_sha} ({})", if is_remote { builder } else { "local" });
    build_runner.run(&build_script(&pc, &git_state, &relpath, &tag, is_remote, &target))?;

    println!("\n[push] {tag} -> {target_str}");
    build_runner.run(&format!("docker pussh {} {}", shq(&tag), shq(&target.pussh_dest())))?;

    let swap_runner = Runner::Remote { host: SshHost::parse(target_str)?, forward_agent: false };
    if let Some(env_file) = &pc.deploy.run.env_file {
        let local_path = root.join(env_file);
        if !local_path.is_file() {
            bail!("env_file {} not found", local_path.display());
        }
        swap_runner.run(&format!("mkdir -p \"$HOME\"/.phost/env"))?;
        let mut scp = Command::new("scp");
        scp.args(["-o", "ControlMaster=auto", "-o", "ControlPath=~/.ssh/phost-%C", "-o", "ControlPersist=10m", "-q"]);
        if let Some(p) = target.port {
            scp.args(["-P", &p.to_string()]);
        }
        scp.arg(&local_path).arg(format!("{}:.phost/env/{name}.env", target.user_host));
        if !scp.status()?.success() {
            bail!("scp of env_file failed");
        }
    }

    println!("\n[swap] {name} on {target_str}");
    swap_runner.run(&swap_script(
        name,
        &repo,
        &tag,
        &run_args(&pc, name, &tag),
        pc.deploy.health.as_ref(),
        true,
    ))?;

    println!("\nDeployed {name} ({tag}) to {target_str}");
    Ok(())
}

fn load_target(conf: &Conf) -> Result<(ProjectConf, String, Runner)> {
    let (pc, _root) = find_project_conf()?;
    let target_str = pc
        .deploy
        .target
        .clone()
        .or_else(|| conf.docker.as_ref().and_then(|d| d.target.clone()))
        .context("no deploy target configured (.phost.toml [deploy] or conf.toml [docker])")?;
    let runner = Runner::Remote { host: SshHost::parse(&target_str)?, forward_agent: false };
    Ok((pc, target_str, runner))
}

pub fn rollback(conf: &Conf) -> Result<()> {
    let (pc, target_str, runner) = load_target(conf)?;
    let name = &pc.app.name;
    let repo = pc.build.image.split(':').next().unwrap();

    let query = format!(
        r#"CUR=$(docker inspect --format '{{{{.Config.Image}}}}' {name} 2>/dev/null || true)
echo "CUR:$CUR"
echo "PREV:$(docker images {repo} --format '{{{{.Repository}}}}:{{{{.Tag}}}}' | grep -vx -- "$CUR" | head -1)""#,
        name = shq(name),
        repo = shq(repo),
    );
    let out = runner.output(&query)?;
    let field = |p: &str| {
        out.lines().find_map(|l| l.strip_prefix(p)).unwrap_or("").trim().to_string()
    };
    let (cur, prev) = (field("CUR:"), field("PREV:"));
    if prev.is_empty() {
        bail!("no previous image retained for {repo} on {target_str}");
    }

    println!(
        "[rollback] {name}: {} -> {prev} on {target_str}",
        if cur.is_empty() { "(not running)" } else { &cur }
    );
    runner.run(&swap_script(
        name,
        repo,
        &prev,
        &run_args(&pc, name, &prev),
        pc.deploy.health.as_ref(),
        false,
    ))?;

    println!("\nRolled back {name} to {prev}");
    Ok(())
}

pub fn status(conf: &Conf) -> Result<()> {
    let (pc, target_str, runner) = load_target(conf)?;
    let name = &pc.app.name;
    let repo = pc.build.image.split(':').next().unwrap();

    let script = format!(
        r#"docker inspect -f 'image:    {{{{.Config.Image}}}}
state:    {{{{.State.Status}}}} (restarts: {{{{.RestartCount}}}})
started:  {{{{.State.StartedAt}}}}' {name} 2>/dev/null || echo 'not deployed'
echo 'images:'
docker images {repo} --format '  {{{{.Repository}}}}:{{{{.Tag}}}}  {{{{.Size}}}}  created {{{{.CreatedSince}}}}'"#,
        name = shq(name),
        repo = shq(repo),
    );
    println!("{name} @ {target_str}");
    println!("{}", runner.output(&script)?);
    Ok(())
}

pub fn logs(conf: &Conf, follow: bool, tail: u32) -> Result<()> {
    let (pc, _, runner) = load_target(conf)?;
    runner.run(&format!(
        "docker logs --tail {tail}{} {}",
        if follow { " -f" } else { "" },
        shq(&pc.app.name)
    ))
}
