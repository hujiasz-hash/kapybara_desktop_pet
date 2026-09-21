//! pi 事件通道：pi 扩展 → 桌宠（仅本机回环 HTTP :17898）
//! 移植自原 main.js 的 startPiEventChannel / onPiEvent。

use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::answer;

pub const PI_EVENT_PORT: u16 = 17898;

/// pi 事件 → 动画映射（与原 PI_EVENT_ANIM 一致）
fn anim_for(ev_type: &str, ok: Option<bool>) -> Option<&'static str> {
    let anim = match ev_type {
        "session_start" => "heart",
        "agent_start" => "thinking",
        "tool_execution_start" => "typing",
        "tool_execution_end" => {
            return Some(if ok == Some(false) { "angry_zen" } else { "inspiration" });
        }
        "agent_settled" => "cheering",
        "session_compact" => "spacing_out",
        _ => return None,
    };
    Some(anim)
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        match TcpListener::bind(("127.0.0.1", PI_EVENT_PORT)).await {
            Err(e) => {
                // 端口被占不影响桌宠本身；pi 扩展的 POST 会静默失败
                println!("[kapybara-buddy] pi 事件端口 {} 不可用（{}）—— 桌宠照常运行", PI_EVENT_PORT, e);
            }
            Ok(listener) => {
                println!(
                    "[kapybara-buddy] pi 事件通道就绪: http://127.0.0.1:{}/pi-event（仅本机）",
                    PI_EVENT_PORT
                );
                loop {
                    match listener.accept().await {
                        Err(_) => continue,
                        Ok((stream, _)) => {
                            let app2 = app.clone();
                            tokio::spawn(async move {
                                handle_conn(app2, stream).await;
                            });
                        }
                    }
                }
            }
        }
    });
}

async fn handle_conn(app: AppHandle, mut stream: TcpStream) {
    // 读请求头（最多 16KB）
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut tmp = [0u8; 1024];
    let head_end;
    loop {
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
        }
        if let Some(i) = find_subslice(&buf, b"\r\n\r\n") {
            head_end = i + 4;
            break;
        }
        if buf.len() > 16384 {
            return;
        }
    }
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or("").to_string();
    let mut content_length: usize = 0;
    for line in lines {
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    if !request_line.starts_with("POST /pi-event") {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }

    // 读 body（上限 16KB，与原逻辑一致）
    let mut body: Vec<u8> = buf[head_end..].to_vec();
    while body.len() < content_length {
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
        }
        if body.len() > 16384 {
            body.truncate(16384);
            break;
        }
    }

    let parsed = serde_json::from_slice::<serde_json::Value>(&body).ok();
    let resp: &[u8] = if parsed.is_some() {
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}"
    } else {
        b"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"ok\":false}"
    };
    if let Some(v) = parsed {
        on_pi_event(&app, &v);
    }
    let _ = stream.write_all(resp).await;
    let _ = stream.flush().await;
}

fn on_pi_event(app: &AppHandle, ev: &serde_json::Value) {
    let ev_type = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
    if ev_type.is_empty() {
        return;
    }
    let ok = ev.get("ok").and_then(|o| o.as_bool());
    let cwd = ev.get("cwd").and_then(|c| c.as_str()).unwrap_or("");
    let Some(anim) = anim_for(ev_type, ok) else { return };
    if cwd.is_empty() {
        println!("[kapybara-buddy] pi 事件 {} → 动画 {}", ev_type, anim);
    } else {
        println!("[kapybara-buddy] pi 事件 {} @ {} → 动画 {}", ev_type, cwd, anim);
    }
    answer::pet_event(app, anim, None);
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
