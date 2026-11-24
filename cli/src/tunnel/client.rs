use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use clap::Args;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use super::{
    decode_frame, encode_frame, find_head_end, first_line_parts, header_value, ControlMsg,
    StreamMsg, CONTROL_PATH, MAX_HEAD_LEN, OP_DATA, OP_EOF, OP_OPEN, OP_RESET,
};
use crate::config::Conf;

#[derive(Args)]
pub struct TunnelArgs {
    /// Local port to expose
    pub port: u16,
    /// Subdomain to serve the tunnel on.  If left off, a random git-short-hash-style subdomain
    /// is picked.
    #[arg(short = 's', long)]
    pub subdomain: Option<String>,
    /// Disable automatic CORS header injection and preflight handling
    #[arg(long)]
    pub no_cors: bool,
    /// Rewrite the Host header to 127.0.0.1:<port>; needed for local dev servers that validate
    /// Host (e.g. vite's allowedHosts)
    #[arg(long)]
    pub rewrite_host: bool,
    #[arg(long, hide = true)]
    pub control_url: Option<String>,
}

pub(super) struct StreamOpts {
    pub port: u16,
    pub cors: bool,
    pub rewrite_host: bool,
}

pub fn run(conf: &Conf, args: TunnelArgs) -> Result<()> {
    let subdomain = args.subdomain.clone().unwrap_or_else(random_subdomain);
    let control_url = args.control_url.clone().unwrap_or_else(|| {
        format!(
            "{}://tunnel.p.{}{}",
            if conf.hosting_protocol == "https" { "wss" } else { "ws" },
            conf.hosting_base_url,
            CONTROL_PATH
        )
    });
    let opts =
        Arc::new(StreamOpts { port: args.port, cors: !args.no_cors, rewrite_host: args.rewrite_host });

    tokio::runtime::Runtime::new()?.block_on(run_client(
        conf.username.clone(),
        conf.password.clone(),
        control_url,
        subdomain,
        opts,
    ))
}

fn random_subdomain() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..7].to_string()
}

enum Outcome {
    Lost,
    Rejected(String),
}

pub(super) async fn run_client(
    username: String,
    password: String,
    control_url: String,
    subdomain: String,
    opts: Arc<StreamOpts>,
) -> Result<()> {
    let mut backoff = 1u64;
    loop {
        match session(&username, &password, &control_url, &subdomain, &opts).await {
            Ok(Outcome::Rejected(message)) => bail!("Tunnel registration rejected: {message}"),
            Ok(Outcome::Lost) => {
                backoff = 1;
                eprintln!("Tunnel connection lost; reconnecting...");
            }
            Err(e) => eprintln!("Tunnel connection failed ({e:#}); retrying in {backoff}s"),
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn session(
    username: &str,
    password: &str,
    control_url: &str,
    subdomain: &str,
    opts: &Arc<StreamOpts>,
) -> Result<Outcome> {
    let (ws, _) =
        connect_async(control_url).await.context("failed to connect to tunnel server")?;
    let (mut ws_tx, mut ws_rx) = ws.split();

    let reg = ControlMsg::Register {
        username: username.to_string(),
        password: password.to_string(),
        subdomain: subdomain.to_string(),
    };
    ws_tx.send(Message::text(serde_json::to_string(&reg)?)).await?;

    let ack = match timeout(Duration::from_secs(10), ws_rx.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str::<ControlMsg>(t.as_str())?,
        Ok(Some(Err(e))) => return Err(e.into()),
        Ok(_) => bail!("connection closed during registration"),
        Err(_) => bail!("registration timed out"),
    };
    match ack {
        ControlMsg::Registered { url } => {
            println!("Tunnel active: {url} -> http://localhost:{}", opts.port);
        }
        ControlMsg::Error { message } => return Ok(Outcome::Rejected(message)),
        _ => bail!("unexpected registration response"),
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });
    let ping_tx = tx.clone();
    let pinger = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(25)).await;
            if ping_tx.send(Message::Ping(Bytes::new())).is_err() {
                break;
            }
        }
    });

    let mut streams: HashMap<u32, mpsc::UnboundedSender<StreamMsg>> = HashMap::new();
    while let Some(msg) = ws_rx.next().await {
        let Ok(msg) = msg else { break };
        match msg {
            Message::Binary(b) => {
                let Some((stream_id, op, payload)) = decode_frame(&b) else {
                    continue;
                };
                match op {
                    OP_OPEN => {
                        let (stream_tx, stream_rx) = mpsc::unbounded_channel();
                        streams.insert(stream_id, stream_tx);
                        tokio::spawn(handle_stream(
                            stream_id,
                            payload,
                            stream_rx,
                            tx.clone(),
                            opts.clone(),
                        ));
                    }
                    OP_DATA => {
                        if let Some(stream_tx) = streams.get(&stream_id) {
                            if stream_tx.send(StreamMsg::Data(payload)).is_err() {
                                streams.remove(&stream_id);
                            }
                        }
                    }
                    OP_EOF | OP_RESET => {
                        if let Some(stream_tx) = streams.remove(&stream_id) {
                            let msg =
                                if op == OP_EOF { StreamMsg::Eof } else { StreamMsg::Reset };
                            let _ = stream_tx.send(msg);
                        }
                    }
                    _ => {}
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    pinger.abort();
    Ok(Outcome::Lost)
}

async fn handle_stream(
    stream_id: u32,
    initial: Bytes,
    mut rx: mpsc::UnboundedReceiver<StreamMsg>,
    tx: mpsc::UnboundedSender<Message>,
    opts: Arc<StreamOpts>,
) {
    let send = |op: u8, payload: &[u8]| {
        tx.send(Message::binary(encode_frame(stream_id, op, payload))).is_ok()
    };

    let mut buf = initial.to_vec();
    let head_end = loop {
        if let Some(end) = find_head_end(&buf) {
            break end;
        }
        if buf.len() > MAX_HEAD_LEN {
            send(OP_RESET, &[]);
            return;
        }
        match rx.recv().await {
            Some(StreamMsg::Data(b)) => buf.extend_from_slice(&b),
            _ => {
                send(OP_RESET, &[]);
                return;
            }
        }
    };

    let head = &buf[..head_end];
    let (method, path) = first_line_parts(head);
    let origin = header_value(head, "origin");

    if opts.cors && method == "OPTIONS" {
        if let Some(req_method) = header_value(head, "access-control-request-method") {
            let req_headers = header_value(head, "access-control-request-headers");
            let resp = preflight_response(origin.as_deref(), &req_method, req_headers.as_deref());
            send(OP_DATA, resp.as_bytes());
            send(OP_EOF, &[]);
            println!("{method} {path} -> 204 (preflight)");
            return;
        }
    }

    let mut local = match TcpStream::connect(("127.0.0.1", opts.port)).await {
        Ok(s) => s,
        Err(e) => {
            let body = format!("phost tunnel: failed to connect to localhost:{}: {e}\n", opts.port);
            let resp = format!(
                "HTTP/1.1 502 Bad Gateway\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            send(OP_DATA, resp.as_bytes());
            send(OP_EOF, &[]);
            println!("{method} {path} -> 502 (local connect failed)");
            return;
        }
    };
    let _ = local.set_nodelay(true);

    let head_bytes =
        if opts.rewrite_host { rewrite_host(head, opts.port) } else { head.to_vec() };
    if local.write_all(&head_bytes).await.is_err()
        || local.write_all(&buf[head_end..]).await.is_err()
    {
        send(OP_RESET, &[]);
        return;
    }

    let (mut local_rd, mut local_wr) = local.split();
    let mut resp_buf: Vec<u8> = Vec::new();
    let mut resp_head_done = false;
    let mut tunnel_open = true;
    let mut chunk = vec![0u8; 16384];

    loop {
        tokio::select! {
            msg = rx.recv(), if tunnel_open => match msg {
                Some(StreamMsg::Data(b)) => {
                    if local_wr.write_all(&b).await.is_err() {
                        send(OP_RESET, &[]);
                        break;
                    }
                }
                Some(StreamMsg::Eof) => {
                    let _ = local_wr.shutdown().await;
                    tunnel_open = false;
                }
                Some(StreamMsg::Reset) | None => break,
            },
            res = local_rd.read(&mut chunk) => {
                let n = match res {
                    Ok(n) => n,
                    Err(_) => {
                        send(OP_RESET, &[]);
                        break;
                    }
                };
                if n == 0 {
                    if !resp_head_done && !resp_buf.is_empty() {
                        send(OP_DATA, &resp_buf);
                    }
                    send(OP_EOF, &[]);
                    break;
                }
                if resp_head_done {
                    if !send(OP_DATA, &chunk[..n]) {
                        break;
                    }
                    continue;
                }
                resp_buf.extend_from_slice(&chunk[..n]);
                if let Some(end) = find_head_end(&resp_buf) {
                    let (_, status) = first_line_parts(&resp_buf[..end]);
                    let patched = if opts.cors {
                        inject_cors(&resp_buf[..end], origin.as_deref())
                    } else {
                        resp_buf[..end].to_vec()
                    };
                    let body_started = resp_buf.len() > end;
                    if !send(OP_DATA, &patched) || (body_started && !send(OP_DATA, &resp_buf[end..])) {
                        break;
                    }
                    resp_head_done = true;
                    resp_buf = Vec::new();
                    println!("{method} {path} -> {status}");
                } else if resp_buf.len() > MAX_HEAD_LEN {
                    // Not parseable as HTTP; give up on header injection and forward raw
                    if !send(OP_DATA, &resp_buf) {
                        break;
                    }
                    resp_head_done = true;
                    resp_buf = Vec::new();
                }
            }
        }
    }
}

fn preflight_response(
    origin: Option<&str>,
    req_method: &str,
    req_headers: Option<&str>,
) -> String {
    let mut r = String::from("HTTP/1.1 204 No Content\r\n");
    r += &cors_headers(origin);
    r += &format!(
        "Access-Control-Allow-Methods: {req_method}, GET, POST, PUT, PATCH, DELETE, OPTIONS, HEAD\r\n"
    );
    if let Some(h) = req_headers {
        r += &format!("Access-Control-Allow-Headers: {h}\r\n");
    }
    r += "Access-Control-Max-Age: 86400\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    r
}

fn cors_headers(origin: Option<&str>) -> String {
    match origin {
        Some(o) => format!(
            "Access-Control-Allow-Origin: {o}\r\nAccess-Control-Allow-Credentials: true\r\nVary: Origin\r\n"
        ),
        None => "Access-Control-Allow-Origin: *\r\n".to_string(),
    }
}

fn inject_cors(head: &[u8], origin: Option<&str>) -> Vec<u8> {
    if header_value(head, "access-control-allow-origin").is_some() {
        return head.to_vec();
    }
    let mut out = head[..head.len() - 2].to_vec();
    out.extend_from_slice(cors_headers(origin).as_bytes());
    out.extend_from_slice(b"\r\n");
    out
}

fn rewrite_host(head: &[u8], port: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(head.len() + 16);
    let mut rest = head;
    while let Some(pos) = rest.windows(2).position(|w| w == b"\r\n") {
        let line = &rest[..pos];
        if line.len() >= 5 && line[..5].eq_ignore_ascii_case(b"host:") {
            out.extend_from_slice(format!("Host: 127.0.0.1:{port}").as_bytes());
        } else {
            out.extend_from_slice(line);
        }
        out.extend_from_slice(b"\r\n");
        rest = &rest[pos + 2..];
    }
    out
}
