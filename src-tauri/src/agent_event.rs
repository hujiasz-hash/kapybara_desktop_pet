//! Agent 事件通道与多 Agent 会话聚合引擎
//!
//! 支持接入：
//!   - Antigravity (hooks.json)
//!   - Claude Code (settings.json hooks)
//!   - pi (kapybara-bridge.ts 扩展)
//!   - 任意本地 Agent / 脚本（POST http://127.0.0.1:17898/agent-event）
//!
//! 解决多 Agent 并发冲突：
//!   1. 早退误休眠：当某个 Agent 提前 settle/cheer，若其他 Agent 仍在 typing/thinking，
//!      自动计算并传递全局聚合 fallback，欢呼完回到真实打工态，绝不早退睡觉。
//!   2. 能级仲裁：任意 Agent 在执行工具（typing）优先于思考（thinking）。
//!   3. 僵尸会话自愈：维护心跳租约（typing 180s / thinking 120s），超时自动注销，
//!      防止终端 Ctrl+C 强退导致桌宠永久卡在打工态。

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::answer;

pub const EVENT_PORT: u16 = 17898;
#[allow(dead_code)]
pub const PI_EVENT_PORT: u16 = EVENT_PORT; // 保持向后兼容

const TTL_TYPING: Duration = Duration::from_secs(180);
const TTL_THINKING: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkState {
    Thinking,
    Typing,
    Settled,
    #[allow(dead_code)]
    Closed,
}

#[derive(Debug, Clone)]
pub struct SessionEntry {
    #[allow(dead_code)]
    pub source: String,
    pub state: WorkState,
    pub last_active: Instant,
}

pub struct SessionTracker {
    sessions: HashMap<String, SessionEntry>,
}

impl SessionTracker {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// 清理超时失效的僵尸会话
    pub fn cleanup_stale(&mut self) {
        let now = Instant::now();
        self.sessions.retain(|_id, entry| {
            let elapsed = now.duration_since(entry.last_active);
            match entry.state {
                WorkState::Typing => elapsed < TTL_TYPING,
                WorkState::Thinking => elapsed < TTL_THINKING,
                WorkState::Settled | WorkState::Closed => elapsed < Duration::from_secs(30),
            }
        });
    }

    /// 更新会话状态并返回 (当前活跃会话数, 全局聚合工作态)
    pub fn update(
        &mut self,
        session_id: &str,
        source: &str,
        ev_norm: &str,
    ) -> (usize, Option<&'static str>) {
        self.cleanup_stale();
        let now = Instant::now();

        match ev_norm {
            "agent_start" => {
                self.sessions.insert(
                    session_id.to_string(),
                    SessionEntry {
                        source: source.to_string(),
                        state: WorkState::Thinking,
                        last_active: now,
                    },
                );
            }
            "tool_execution_start" => {
                self.sessions.insert(
                    session_id.to_string(),
                    SessionEntry {
                        source: source.to_string(),
                        state: WorkState::Typing,
                        last_active: now,
                    },
                );
            }
            "tool_execution_end" => {
                if let Some(entry) = self.sessions.get_mut(session_id) {
                    entry.last_active = now;
                    // 工具执行完毕后，Agent 仍处于该轮活跃中
                } else {
                    self.sessions.insert(
                        session_id.to_string(),
                        SessionEntry {
                            source: source.to_string(),
                            state: WorkState::Typing,
                            last_active: now,
                        },
                    );
                }
            }
            "agent_settled" => {
                if let Some(entry) = self.sessions.get_mut(session_id) {
                    entry.state = WorkState::Settled;
                    entry.last_active = now;
                }
            }
            "session_shutdown" => {
                self.sessions.remove(session_id);
            }
            _ => {
                // 瞬时事件（如 heart, spacing_out 等）仅刷新活跃时间
                if let Some(entry) = self.sessions.get_mut(session_id) {
                    entry.last_active = now;
                }
            }
        }

        // 计算当前全局聚合状态
        let mut active_count = 0;
        let mut has_typing = false;
        let mut has_thinking = false;

        for entry in self.sessions.values() {
            match entry.state {
                WorkState::Typing => {
                    active_count += 1;
                    has_typing = true;
                }
                WorkState::Thinking => {
                    active_count += 1;
                    has_thinking = true;
                }
                WorkState::Settled | WorkState::Closed => {}
            }
        }

        let fallback = if has_typing {
            Some("typing")
        } else if has_thinking {
            Some("thinking")
        } else {
            None
        };

        (active_count, fallback)
    }
}

static TRACKER: LazyLock<Arc<Mutex<SessionTracker>>> =
    LazyLock::new(|| Arc::new(Mutex::new(SessionTracker::new())));

/// 标准化事件名 → 对应动画帧名
fn anim_for(ev_norm: &str, ok: Option<bool>) -> Option<&'static str> {
    match ev_norm {
        "session_start" => Some("heart"),
        "agent_start" => Some("thinking"),
        "tool_execution_start" => Some("typing"),
        "tool_execution_end" => {
            Some(if ok == Some(false) { "angry_zen" } else { "inspiration" })
        }
        "agent_settled" => Some("cheering"),
        "session_compact" => Some("spacing_out"),
        "session_shutdown" => Some("sleep"),
        _ => None,
    }
}

/// 将不同客户端的事件名称归一化
fn normalize_event_type(raw: &str) -> &'static str {
    match raw {
        "session_start" | "SessionStart" => "session_start",
        "agent_start" | "PreInvocation" | "UserPromptSubmit" => "agent_start",
        "tool_execution_start" | "PreToolUse" => "tool_execution_start",
        "tool_execution_end" | "PostToolUse" => "tool_execution_end",
        "PostToolUseFailure" => "tool_execution_end",
        "agent_settled" | "Stop" => "agent_settled",
        "session_compact" | "PreCompact" => "session_compact",
        "session_shutdown" | "SessionEnd" => "session_shutdown",
        _ => "unknown",
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        match TcpListener::bind(("127.0.0.1", EVENT_PORT)).await {
            Err(e) => {
                println!(
                    "[kapybara-buddy] Agent 事件端口 {} 不可用（{}）—— 桌宠照常运行",
                    EVENT_PORT, e
                );
            }
            Ok(listener) => {
                println!(
                    "[kapybara-buddy] Agent 事件通道就绪: http://127.0.0.1:{}/agent-event（仅本机）",
                    EVENT_PORT
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

    if !request_line.starts_with("POST ") {
        let _ = stream
            .write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await;
        return;
    }

    // 读 body（上限 32KB）
    let mut body: Vec<u8> = buf[head_end..].to_vec();
    while body.len() < content_length {
        match stream.read(&mut tmp).await {
            Ok(0) | Err(_) => break,
            Ok(n) => body.extend_from_slice(&tmp[..n]),
        }
        if body.len() > 32768 {
            body.truncate(32768);
            break;
        }
    }

    let (query_event, query_source) = parse_query_params(&request_line);

    let parsed = serde_json::from_slice::<serde_json::Value>(&body).ok();
    if let Some(ref v) = parsed {
        on_agent_event(&app, v, query_event.as_deref(), query_source.as_deref());
    }

    let resp: &[u8] = if parsed.is_some() {
        b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}"
    } else {
        b"HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n{\"ok\":false}"
    };
    let _ = stream.write_all(resp).await;
    let _ = stream.flush().await;
}

fn parse_query_params(request_line: &str) -> (Option<String>, Option<String>) {
    let mut ev = None;
    let mut src = None;
    let path = request_line.split_whitespace().nth(1).unwrap_or("");
    if let Some(pos) = path.find('?') {
        let query = &path[pos + 1..];
        for pair in query.split('&') {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next().unwrap_or("");
            let v = parts.next().unwrap_or("");
            if k == "event" || k == "type" {
                ev = Some(v.to_string());
            } else if k == "source" {
                src = Some(v.to_string());
            }
        }
    }
    (ev, src)
}

fn on_agent_event(
    app: &AppHandle,
    ev: &serde_json::Value,
    query_event: Option<&str>,
    query_source: Option<&str>,
) {
    // 提取来源：antigravity / claude / pi / custom
    let source = ev
        .get("source")
        .and_then(|s| s.as_str())
        .or(query_source)
        .unwrap_or_else(|| {
            if ev.get("conversationId").is_some() {
                "antigravity"
            } else if ev.get("session_id").is_some() {
                "claude"
            } else {
                "pi"
            }
        });

    // 提取会话唯一 ID
    let session_id = ev
        .get("sessionId")
        .or_else(|| ev.get("session_id"))
        .or_else(|| ev.get("conversationId"))
        .or_else(|| ev.get("cwd"))
        .and_then(|id| id.as_str())
        .unwrap_or("default-session");

    // 提取事件名并归一化
    let raw_type = ev
        .get("type")
        .or_else(|| ev.get("event"))
        .or_else(|| ev.get("hook_event_name"))
        .or_else(|| ev.get("hookEventName"))
        .and_then(|t| t.as_str())
        .or(query_event)
        .unwrap_or("");

    let ev_norm = normalize_event_type(raw_type);
    if ev_norm == "unknown" {
        return;
    }

    // 判断工具成败
    let is_failure_type = raw_type == "PostToolUseFailure";
    let has_error = ev.get("error").map(|e| !e.is_null() && e != "").unwrap_or(false);
    let explicit_ok = ev.get("ok").and_then(|o| o.as_bool());
    let ok = if is_failure_type || has_error {
        Some(false)
    } else if let Some(o) = explicit_ok {
        Some(o)
    } else {
        Some(true)
    };

    let Some(anim) = anim_for(ev_norm, ok) else {
        return;
    };

    // 会话引擎更新与仲裁
    let (active_count, fallback) = {
        let mut tracker = TRACKER.lock().unwrap();
        tracker.update(session_id, source, ev_norm)
    };

    println!(
        "[kapybara-buddy] [{}] 会话 {} -> 事件 {} => 动效 {} (活跃会话: {}, 聚合fallback: {:?})",
        source,
        truncate_id(session_id),
        raw_type,
        anim,
        active_count,
        fallback
    );

    answer::pet_event_full(app, anim, None, fallback, active_count);
}

fn truncate_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}
