pub mod client;
pub mod server;
#[cfg(test)]
mod tests;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

pub const CONTROL_PATH: &str = "/__phost_control";
pub const MAX_HEAD_LEN: usize = 64 * 1024;

pub const OP_OPEN: u8 = 0;
pub const OP_DATA: u8 = 1;
pub const OP_EOF: u8 = 2;
pub const OP_RESET: u8 = 3;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlMsg {
    Register { username: String, password: String, subdomain: String },
    Registered { url: String },
    Error { message: String },
}

pub enum StreamMsg {
    Data(Bytes),
    Eof,
    Reset,
}

pub fn encode_frame(stream_id: u32, op: u8, payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.extend_from_slice(&stream_id.to_be_bytes());
    buf.push(op);
    buf.extend_from_slice(payload);
    buf
}

pub fn decode_frame(frame: &Bytes) -> Option<(u32, u8, Bytes)> {
    if frame.len() < 5 {
        return None;
    }
    let stream_id = u32::from_be_bytes(frame[..4].try_into().unwrap());
    Some((stream_id, frame[4], frame.slice(5..)))
}

pub fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4)
}

pub fn header_value(head: &[u8], name: &str) -> Option<String> {
    for line in head.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(colon) = line.iter().position(|&b| b == b':') else {
            continue;
        };
        let (key, value) = line.split_at(colon);
        let Ok(key) = std::str::from_utf8(key) else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case(name) {
            continue;
        }
        let Ok(value) = std::str::from_utf8(&value[1..]) else {
            continue;
        };
        return Some(value.trim().to_string());
    }
    None
}

/// Method + path from a request head, or protocol version + status from a response head
pub fn first_line_parts(head: &[u8]) -> (String, String) {
    let line = head.split(|&b| b == b'\r').next().unwrap_or(b"");
    let mut it = std::str::from_utf8(line).unwrap_or("").split_whitespace();
    (
        it.next().unwrap_or("").to_string(),
        it.next().unwrap_or("").to_string(),
    )
}
