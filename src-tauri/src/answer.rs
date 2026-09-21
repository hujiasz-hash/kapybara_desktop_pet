//! 问答后端：pollinations（免费网关）/ agy（Antigravity CLI 流式）
//! 移植自原 main.js 的 runPollinations / runAgy，行为保持一致。

use std::path::PathBuf;
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncReadExt;

use crate::state::{Active, AppState};

const ANSWER_TIMEOUT_MS: u64 = 180_000;

#[derive(Clone, Copy, PartialEq)]
pub enum Backend {
    Pollinations,
    Agy,
}

pub fn current_backend() -> Backend {
    match std::env::var("KB_BACKEND").as_deref() {
        Ok("agy") => Backend::Agy,
        _ => Backend::Pollinations,
    }
}

pub fn backend_desc() -> String {
    match current_backend() {
        Backend::Pollinations => "pollinations (免费网关，约1~2秒)".into(),
        Backend::Agy => format!("agy ({})", find_agy().map(path_display).unwrap_or_else(|| "未找到!".into())),
    }
}

fn path_display(p: PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

// ---------- 事件辅助 ----------

pub fn send_chunk(app: &AppHandle, text: &str) {
    app.state::<AppState>().test_buf.lock().unwrap().push_str(text);
    let _ = app.emit_to("chat", "answer-chunk", text.to_string());
}

pub fn pet_event(app: &AppHandle, ev: &str, ms: Option<u64>) {
    pet_event_full(app, ev, ms, None, 0);
}

pub fn pet_event_full(
    app: &AppHandle,
    ev: &str,
    ms: Option<u64>,
    fallback: Option<&str>,
    active_count: usize,
) {
    let _ = app.emit_to(
        "pet",
        "pet-event",
        serde_json::json!({
            "e": ev,
            "ms": ms,
            "fallback": fallback,
            "activeCount": active_count,
        }),
    );
}

/// 回答完成：解锁 busy、通知前端、桌宠欢呼、4 秒后自动隐藏（未聚焦时）
pub fn finish(app: &AppHandle) {
    let st = app.state::<AppState>();
    st.busy.store(false, std::sync::atomic::Ordering::SeqCst);
    st.test_done.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    drop(st);
    let _ = app.emit_to("chat", "answer-done", ());
    pet_event(app, "cheering", None);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(4000)).await;
        if let Some(chat) = app2.get_webview_window("chat") {
            if chat.is_visible().unwrap_or(false) && !chat.is_focused().unwrap_or(true) {
                let _ = chat.hide();
            }
        }
    });
}

/// 提问入口（busy 已在 ask 命令里置位）
pub fn launch(app: &AppHandle, question: String) {
    let st = app.state::<AppState>();
    match current_backend() {
        Backend::Pollinations => {
            let a = app.clone();
            let h = tauri::async_runtime::spawn(async move { pollinations_run(a, question).await });
            *st.active.lock().unwrap() = Some(Active::Pollinations(h));
        }
        Backend::Agy => {
            let a = app.clone();
            tauri::async_runtime::spawn(async move { agy_run(a, question).await });
            *st.active.lock().unwrap() = Some(Active::Agy);
        }
    }
}

/// 中止当前回答（对应原 stop：杀子进程 → close → done；abort pollinations → 手动 done）
pub async fn stop(app: AppHandle) {
    let st = app.state::<AppState>();
    let active = st.active.lock().unwrap().take();
    match active {
        Some(Active::Pollinations(h)) => {
            h.abort();
            finish(&app);
        }
        Some(Active::Agy) => {
            let child = st.child.lock().unwrap().take();
            if let Some(mut c) = child {
                let _ = c.kill().await;
                // kill 后 stdout EOF，读循环自然收尾并调用 finish
            } else {
                finish(&app);
            }
        }
        None => {}
    }
}

// ---------- pollinations ----------

async fn pollinations_run(app: AppHandle, question: String) {
    let result: Result<String, String> = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| e.to_string())?;
        let url = format!("https://text.pollinations.ai/{}", utf8_percent_encode(&question, NON_ALPHANUMERIC));
        let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        if !status.is_success() {
            return Err(format!("HTTP {}", status.as_u16()));
        }
        resp.text().await.map_err(|e| e.to_string())
    }
    .await;
    match result {
        Ok(text) => {
            let t = text.trim();
            send_chunk(&app, if t.is_empty() { "[空回答]" } else { t });
        }
        Err(e) => send_chunk(&app, &format!("[出错] {}（后端: pollinations）\n", e)),
    }
    finish(&app);
}

// ---------- agy ----------

fn find_agy() -> Option<PathBuf> {
    // PATH 里找
    let which = if cfg!(target_os = "macos") { "which" } else { "where" };
    if let Ok(out) = std::process::Command::new(which).arg("agy").output() {
        if out.status.success() {
            if let Ok(s) = String::from_utf8(out.stdout) {
                if let Some(first) = s.lines().next() {
                    let p = PathBuf::from(first.trim());
                    if p.exists() {
                        return Some(p);
                    }
                }
            }
        }
    }
    // 常见位置兜底
    let home = std::env::var("HOME").unwrap_or_default();
    #[cfg(target_os = "macos")]
    let candidates = vec![
        PathBuf::from("/opt/homebrew/bin/agy"),
        PathBuf::from("/usr/local/bin/agy"),
        PathBuf::from(&home).join(".local/bin/agy"),
    ];
    #[cfg(windows)]
    let candidates = vec![
        PathBuf::from(&home).join("AppData/Local/Programs/antigravity-cli/agy.exe"),
        PathBuf::from(&home).join(".local/bin/agy.exe"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

/// 跨 read 边界的增量 UTF-8 解码（Node setEncoding('utf8') 的等价物，避免中文被 chunk 切坏）
#[derive(Default)]
struct Utf8Feeder {
    buf: Vec<u8>,
}

impl Utf8Feeder {
    fn push(&mut self, chunk: &[u8]) -> String {
        self.buf.extend_from_slice(chunk);
        match std::str::from_utf8(&self.buf) {
            Ok(s) => {
                let out = s.to_string();
                self.buf.clear();
                out
            }
            Err(e) => {
                let valid = e.valid_up_to();
                let out = String::from_utf8_lossy(&self.buf[..valid]).into_owned();
                self.buf.drain(..valid);
                out
            }
        }
    }
    fn flush(&mut self) -> String {
        if self.buf.is_empty() {
            return String::new();
        }
        let out = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        out
    }
}

async fn agy_run(app: AppHandle, question: String) {
    let model = std::env::var("KB_AGY_MODEL").unwrap_or_else(|_| "gemini-3.8-flash-low".into());
    let Some(agy) = find_agy() else {
        send_chunk(&app, "[未找到 agy 命令] 请先安装 Antigravity CLI 并登录（见 README）\n");
        finish(&app);
        return;
    };

    let mut cmd = tokio::process::Command::new(&agy);
    cmd.arg("--model").arg(&model).arg("-p").arg(&question);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            send_chunk(&app, &format!("\n[调用失败] {}\n", e));
            finish(&app);
            return;
        }
    };
    let mut stdout = child.stdout.take().expect("agy stdout");
    let mut stderr = child.stderr.take().expect("agy stderr");
    *app.state::<AppState>().child.lock().unwrap() = Some(child);

    // stderr 并发收集（仅当无 stdout 输出时使用，对应原逻辑）
    let stderr_task = tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut tmp = [0u8; 2048];
        loop {
            match stderr.read(&mut tmp).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    buf.extend_from_slice(&tmp[..n]);
                    if buf.len() > 16384 {
                        break;
                    }
                }
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    let mut feeder = Utf8Feeder::default();
    let mut got_output = false;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ANSWER_TIMEOUT_MS);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            // 超时：杀进程（对应 spawn timeout），无输出才报错
            if !got_output {
                send_chunk(&app, "\n[出错] agy 超时（180s），可运行 \"agy\" 检查登录状态\n");
            }
            let taken = { app.state::<AppState>().child.lock().unwrap().take() };
            if let Some(mut c) = taken {
                let _ = c.kill().await;
            }
            break;
        }
        let mut buf = [0u8; 4096];
        match tokio::time::timeout(remaining, stdout.read(&mut buf)).await {
            Err(_) => continue, // 到达 deadline，下一轮 remaining=0 收尾
            Ok(Err(_)) => {
                send_chunk(&app, "\n[调用失败] 读取 agy 输出失败\n");
                break;
            }
            Ok(Ok(0)) => break, // EOF
            Ok(Ok(n)) => {
                let s = feeder.push(&buf[..n]);
                if !s.is_empty() {
                    if s.trim().len() > 0 {
                        got_output = true;
                    }
                    send_chunk(&app, &s);
                }
            }
        }
    }
    let tail = feeder.flush();
    if !tail.is_empty() {
        send_chunk(&app, &tail);
    }
    let stderr_text = stderr_task.await.unwrap_or_default();

    // 退出码检查（对应 close 事件）；stop 已把 child 取走时跳过
    let child_taken = { app.state::<AppState>().child.lock().unwrap().take() };
    if let Some(mut c) = child_taken {
        let code = c.wait().await.ok().and_then(|s| s.code());
        if code != Some(0) && !got_output {
            if !stderr_text.trim().is_empty() {
                send_chunk(&app, &stderr_text);
            }
            send_chunk(&app, &format!("\n[出错] agy 退出码 {:?}，可运行 \"agy\" 检查登录状态\n", code));
        }
    }
    finish(&app);
}
