use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

use super::client::{run_client, StreamOpts};
use super::server::{serve, TunnelServerArgs};

async fn read_http_request(sock: &mut TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(end) = super::find_head_end(&buf) {
            break end;
        }
        let n = sock.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            return buf;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let content_length = super::header_value(&buf[..head_end], "content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < head_end + content_length {
        let n = sock.read(&mut chunk).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    buf
}

async fn respond_and_close(sock: &mut TcpStream, status: &str, body: &str) {
    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = sock.write_all(resp.as_bytes()).await;
}

async fn spawn_auth_ok() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                read_http_request(&mut sock).await;
                respond_and_close(&mut sock, "200 OK", r#"{"success": true}"#).await;
            });
        }
    });
    format!("http://{addr}/login/")
}

async fn spawn_hello() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                read_http_request(&mut sock).await;
                respond_and_close(&mut sock, "200 OK", "hello world").await;
            });
        }
    });
    port
}

async fn spawn_ws_echo() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((sock, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let Ok(mut ws) = tokio_tungstenite::accept_async(sock).await else {
                    return;
                };
                while let Some(Ok(msg)) = ws.next().await {
                    match msg {
                        Message::Text(_) | Message::Binary(_) => {
                            if ws.send(msg).await.is_err() {
                                break;
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            });
        }
    });
    port
}

async fn http_via_tunnel(port: u16, raw: &str) -> String {
    let mut sock = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    sock.write_all(raw.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    let _ = timeout(Duration::from_secs(5), sock.read_to_end(&mut out)).await;
    String::from_utf8_lossy(&out).to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn tunnel_e2e() {
    let auth_url = spawn_auth_ok().await;
    let tunnel_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let public_port = tunnel_listener.local_addr().unwrap().port();
    tokio::spawn(serve(
        tunnel_listener,
        TunnelServerArgs { port: 0, auth_url, domain: "p.test".into() },
    ));

    let control_url = format!("ws://127.0.0.1:{public_port}{}", super::CONTROL_PATH);
    let hello_port = spawn_hello().await;
    let ws_port = spawn_ws_echo().await;

    let web_client = tokio::spawn(run_client(
        "u".into(),
        "pw".into(),
        control_url.clone(),
        "web".into(),
        Arc::new(StreamOpts { port: hello_port, cors: true, rewrite_host: false }),
    ));
    let ws_client = tokio::spawn(run_client(
        "u".into(),
        "pw".into(),
        control_url,
        "wsecho".into(),
        Arc::new(StreamOpts { port: ws_port, cors: false, rewrite_host: false }),
    ));

    // Plain HTTP through the tunnel, with CORS injected; polls until registration completes
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let resp = http_via_tunnel(
            public_port,
            "GET / HTTP/1.1\r\nHost: web.p.test\r\nOrigin: https://example.com\r\nConnection: close\r\n\r\n",
        )
        .await;
        if resp.contains("200 OK") && resp.contains("hello world") {
            assert!(resp.contains("Access-Control-Allow-Origin: https://example.com"), "{resp}");
            assert!(resp.contains("Access-Control-Allow-Credentials: true"), "{resp}");
            break;
        }
        assert!(tokio::time::Instant::now() < deadline, "tunnel never came up; last resp: {resp}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Synthetic preflight
    let resp = http_via_tunnel(
        public_port,
        "OPTIONS /api HTTP/1.1\r\nHost: web.p.test\r\nOrigin: https://example.com\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: content-type\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(resp.contains("204 No Content"), "{resp}");
    assert!(resp.contains("Access-Control-Allow-Origin: https://example.com"), "{resp}");
    assert!(resp.contains("Access-Control-Allow-Methods: POST"), "{resp}");
    assert!(resp.contains("Access-Control-Allow-Headers: content-type"), "{resp}");

    // Unknown subdomain
    let resp = http_via_tunnel(
        public_port,
        "GET / HTTP/1.1\r\nHost: nope.p.test\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(resp.contains("404 Not Found"), "{resp}");

    // WebSocket echo through the tunnel; polls until the wsecho tunnel registers
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let tcp = TcpStream::connect(("127.0.0.1", public_port)).await.unwrap();
        match tokio_tungstenite::client_async("ws://wsecho.p.test/ws", tcp).await {
            Ok((mut ws, _)) => {
                ws.send(Message::text("marco")).await.unwrap();
                let echoed = loop {
                    match timeout(Duration::from_secs(5), ws.next()).await {
                        Ok(Some(Ok(Message::Text(t)))) => break t.to_string(),
                        Ok(Some(Ok(_))) => continue,
                        other => panic!("ws echo failed: {other:?}"),
                    }
                };
                assert_eq!(echoed, "marco");
                let _ = ws.close(None).await;
                break;
            }
            Err(_) => {
                assert!(tokio::time::Instant::now() < deadline, "ws tunnel never came up");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    web_client.abort();
    ws_client.abort();
}
