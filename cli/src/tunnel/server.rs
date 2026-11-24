use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use clap::Args;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use super::{
    decode_frame, encode_frame, find_head_end, header_value, ControlMsg, StreamMsg, CONTROL_PATH,
    MAX_HEAD_LEN, OP_DATA, OP_EOF, OP_OPEN, OP_RESET,
};

#[derive(Args)]
pub struct TunnelServerArgs {
    /// Port to listen on; the outer reverse proxy routes *.<domain> and the control endpoint here
    #[arg(long, default_value_t = 4301)]
    pub port: u16,
    /// Login endpoint used to validate tunnel credentials
    #[arg(long, default_value = "http://localhost:4300/login/")]
    pub auth_url: String,
    /// Domain suffix under which tunnels are publicly served
    #[arg(long, default_value = "p.ameo.design")]
    pub domain: String,
}

struct Session {
    tx: mpsc::UnboundedSender<Message>,
    streams: Mutex<HashMap<u32, mpsc::UnboundedSender<StreamMsg>>>,
    next_stream_id: AtomicU32,
}

type Sessions = Arc<Mutex<HashMap<String, Arc<Session>>>>;

pub fn run(args: TunnelServerArgs) -> Result<()> {
    tokio::runtime::Runtime::new()?.block_on(async {
        let listener = TcpListener::bind(("127.0.0.1", args.port)).await?;
        println!("Tunnel server listening on 127.0.0.1:{}", args.port);
        serve(listener, args).await
    })
}

pub(super) async fn serve(listener: TcpListener, args: TunnelServerArgs) -> Result<()> {
    let sessions = Sessions::default();
    let args = Arc::new(args);
    loop {
        match listener.accept().await {
            Ok((sock, _)) => {
                tokio::spawn(handle_conn(sock, sessions.clone(), args.clone()));
            }
            Err(e) => {
                eprintln!("accept error: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn read_head(sock: &mut TcpStream, buf: &mut Vec<u8>) -> std::io::Result<Option<usize>> {
    let mut chunk = [0u8; 8192];
    loop {
        if let Some(end) = find_head_end(buf) {
            return Ok(Some(end));
        }
        if buf.len() > MAX_HEAD_LEN {
            return Ok(None);
        }
        let n = sock.read(&mut chunk).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn handle_conn(mut sock: TcpStream, sessions: Sessions, args: Arc<TunnelServerArgs>) {
    let _ = sock.set_nodelay(true);
    let mut buf = Vec::with_capacity(8192);
    let head_end = match timeout(Duration::from_secs(15), read_head(&mut sock, &mut buf)).await {
        Ok(Ok(Some(end))) => end,
        _ => return,
    };

    let (_, path) = super::first_line_parts(&buf[..head_end]);
    if path.starts_with(CONTROL_PATH) {
        handle_control(sock, &buf[..head_end], sessions, args).await;
    } else {
        handle_public(sock, buf, head_end, sessions).await;
    }
}

fn valid_subdomain(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 63
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

async fn check_auth(auth_url: &str, username: &str, password: &str) -> bool {
    let res = reqwest::Client::new()
        .post(auth_url)
        .form(&[("username", username), ("password", password)])
        .send()
        .await;
    match res {
        Ok(res) if res.status().is_success() => res
            .json::<serde_json::Value>()
            .await
            .map(|v| v["success"].as_bool() == Some(true))
            .unwrap_or(false),
        _ => false,
    }
}

fn reset_all_streams(session: &Session) {
    for (_, tx) in session.streams.lock().unwrap().drain() {
        let _ = tx.send(StreamMsg::Reset);
    }
}

async fn handle_control(
    mut sock: TcpStream,
    head: &[u8],
    sessions: Sessions,
    args: Arc<TunnelServerArgs>,
) {
    let Some(key) = header_value(head, "sec-websocket-key") else {
        // Plain GET with no websocket handshake: health check
        let body = "phost tunnel server\n";
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        return;
    };
    let resp = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        derive_accept_key(key.as_bytes())
    );
    if sock.write_all(resp.as_bytes()).await.is_err() {
        return;
    }
    let ws = WebSocketStream::from_raw_socket(sock, Role::Server, None).await;
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (username, password, subdomain) = match timeout(Duration::from_secs(10), ws_rx.next()).await
    {
        Ok(Some(Ok(Message::Text(t)))) => match serde_json::from_str::<ControlMsg>(t.as_str()) {
            Ok(ControlMsg::Register { username, password, subdomain }) => {
                (username, password, subdomain)
            }
            _ => return,
        },
        _ => return,
    };

    let err = if !valid_subdomain(&subdomain) {
        Some("invalid subdomain: must be lowercase [a-z0-9-], max 63 chars")
    } else if !check_auth(&args.auth_url, &username, &password).await {
        Some("authentication failed")
    } else {
        None
    };
    if let Some(message) = err {
        let msg = ControlMsg::Error { message: message.to_string() };
        let _ = ws_tx.send(Message::text(serde_json::to_string(&msg).unwrap())).await;
        return;
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let session = Arc::new(Session {
        tx,
        streams: Mutex::new(HashMap::new()),
        next_stream_id: AtomicU32::new(1),
    });

    // An authenticated re-registration takes over the subdomain so a reconnecting client isn't
    // locked out while the server hasn't yet noticed its previous socket is dead
    let old = sessions.lock().unwrap().insert(subdomain.clone(), session.clone());
    if let Some(old) = old {
        let _ = old.tx.send(Message::Close(None));
        reset_all_streams(&old);
    }

    let ack = ControlMsg::Registered { url: format!("https://{}.{}/", subdomain, args.domain) };
    if ws_tx.send(Message::text(serde_json::to_string(&ack).unwrap())).await.is_err() {
        sessions.lock().unwrap().remove(&subdomain);
        return;
    }
    println!("tunnel registered: {subdomain}");

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let close = matches!(msg, Message::Close(_));
            if ws_tx.send(msg).await.is_err() || close {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_rx.next().await {
        match msg {
            Message::Binary(b) => {
                let Some((stream_id, op, payload)) = decode_frame(&b) else {
                    continue;
                };
                let sender = session.streams.lock().unwrap().get(&stream_id).cloned();
                let Some(sender) = sender else {
                    continue;
                };
                let msg = match op {
                    OP_DATA => StreamMsg::Data(payload),
                    OP_EOF => StreamMsg::Eof,
                    OP_RESET => StreamMsg::Reset,
                    _ => continue,
                };
                let _ = sender.send(msg);
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    let mut sessions_inner = sessions.lock().unwrap();
    if sessions_inner.get(&subdomain).is_some_and(|s| Arc::ptr_eq(s, &session)) {
        sessions_inner.remove(&subdomain);
    }
    drop(sessions_inner);
    reset_all_streams(&session);
    writer.abort();
    println!("tunnel disconnected: {subdomain}");
}

async fn handle_public(mut sock: TcpStream, buf: Vec<u8>, head_end: usize, sessions: Sessions) {
    let host = header_value(&buf[..head_end], "host").unwrap_or_default();
    let subdomain = host.split(['.', ':']).next().unwrap_or("").to_ascii_lowercase();

    let session = sessions.lock().unwrap().get(&subdomain).cloned();
    let Some(session) = session else {
        let body = format!("No active tunnel for subdomain {subdomain:?}\n");
        let resp = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = sock.write_all(resp.as_bytes()).await;
        return;
    };

    let stream_id = session.next_stream_id.fetch_add(1, Ordering::Relaxed);
    let (stream_tx, mut stream_rx) = mpsc::unbounded_channel();
    session.streams.lock().unwrap().insert(stream_id, stream_tx);
    if session.tx.send(Message::binary(encode_frame(stream_id, OP_OPEN, &buf))).is_err() {
        session.streams.lock().unwrap().remove(&stream_id);
        return;
    }

    let (mut rd, mut wr) = sock.into_split();
    let up_session = session.clone();
    let up = tokio::spawn(async move {
        let mut chunk = [0u8; 16384];
        loop {
            match rd.read(&mut chunk).await {
                Ok(0) => {
                    let _ = up_session.tx.send(Message::binary(encode_frame(stream_id, OP_EOF, &[])));
                    break;
                }
                Ok(n) => {
                    let frame = Message::binary(encode_frame(stream_id, OP_DATA, &chunk[..n]));
                    if up_session.tx.send(frame).is_err() {
                        break;
                    }
                }
                Err(_) => {
                    let _ =
                        up_session.tx.send(Message::binary(encode_frame(stream_id, OP_RESET, &[])));
                    break;
                }
            }
        }
    });

    while let Some(msg) = stream_rx.recv().await {
        match msg {
            StreamMsg::Data(b) => {
                if wr.write_all(&b).await.is_err() {
                    let _ = session.tx.send(Message::binary(encode_frame(stream_id, OP_RESET, &[])));
                    break;
                }
            }
            StreamMsg::Eof => {
                let _ = wr.shutdown().await;
                break;
            }
            StreamMsg::Reset => break,
        }
    }

    session.streams.lock().unwrap().remove(&stream_id);
    up.abort();
}
