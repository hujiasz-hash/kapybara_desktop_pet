//! 会话日志监听：直接增量读取 Claude Code / pi 已经写入磁盘的 jsonl，
//! 不依赖各 CLI 的 hook 配置。新增一个会写日志的 CLI，只需把它的目录加进 ROOTS 并实现 classify。
//!
//! 设计要点（见 /tmp 原型验证记录）：
//!   - 每个文件只保存一个读取偏移量；启动时直接定位到末尾，不回放历史
//!   - 每次只读新增字节，只处理到最后一个换行符为止的完整行，解析后即丢弃
//!   - 只跟踪最近 1 小时修改过的文件，其余忽略
//!   - 轮询 mtime/size（300ms），不引入 FSEvents 依赖
//!
//! 环境变量：
//!   KB_TRANSCRIPT_WATCH=0   完全关闭日志监听
//!   KB_TRANSCRIPT_SHADOW=1  只打印日志监听判定的状态，不驱动桌宠（用于与 hook 对照）

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tauri::AppHandle;

use crate::agent_event;

const POLL: Duration = Duration::from_millis(300);
const SCAN_EVERY: Duration = Duration::from_secs(2);
const RECENT: Duration = Duration::from_secs(3600);
/// 单次读取上限；超过仍无换行的超长残行直接跳过，避免反复重读
const MAX_READ: u64 = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Src {
    Claude,
    Pi,
}

impl Src {
    pub fn name(self) -> &'static str {
        match self {
            Src::Claude => "claude",
            Src::Pi => "pi",
        }
    }
}

/// 日志记录映射出的工作阶段（与 hook 事件的语义一一对应）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Thinking,
    Typing,
    Settled,
}

impl Phase {
    /// 对应 agent_event 里的标准化事件名
    pub fn ev_norm(self) -> &'static str {
        match self {
            Phase::Thinking => "agent_start",
            Phase::Typing => "tool_execution_start",
            Phase::Settled => "agent_settled",
        }
    }
}

/// 单行日志 → 工作阶段。与 Claude Code / pi 的 jsonl 结构对应，无法判定的返回 None
pub fn classify(src: Src, rec: &serde_json::Value) -> Option<Phase> {
    match src {
        Src::Claude => {
            let t = rec.get("type")?.as_str()?;
            let msg = rec.get("message")?;
            match t {
                "user" => {
                    let has_result = msg
                        .get("content")
                        .and_then(|c| c.as_array())
                        .map(|blocks| {
                            blocks
                                .iter()
                                .any(|b| b.get("type").and_then(|x| x.as_str()) == Some("tool_result"))
                        })
                        .unwrap_or(false);
                    Some(if has_result { Phase::Typing } else { Phase::Thinking })
                }
                "assistant" => {
                    let has_tool = block_types(msg).any(|x| x == "tool_use");
                    if has_tool {
                        Some(Phase::Typing)
                    } else if msg.get("stop_reason").and_then(|x| x.as_str()) == Some("end_turn") {
                        Some(Phase::Settled)
                    } else {
                        Some(Phase::Thinking)
                    }
                }
                _ => None,
            }
        }
        Src::Pi => {
            if rec.get("type")?.as_str()? != "message" {
                return None;
            }
            let msg = rec.get("message")?;
            match msg.get("role")?.as_str()? {
                "user" => Some(Phase::Thinking),
                "toolResult" => Some(Phase::Typing),
                "assistant" => {
                    if block_types(msg).any(|x| x == "toolCall" || x == "tool_use") {
                        Some(Phase::Typing)
                    } else if msg.get("stopReason").and_then(|x| x.as_str()) == Some("stop") {
                        Some(Phase::Settled)
                    } else {
                        Some(Phase::Thinking)
                    }
                }
                _ => None,
            }
        }
    }
}

fn block_types(msg: &serde_json::Value) -> impl Iterator<Item = &str> {
    msg.get("content")
        .and_then(|c| c.as_array())
        .into_iter()
        .flatten()
        .filter_map(|b| b.get("type").and_then(|x| x.as_str()))
}

struct Tracked {
    path: PathBuf,
    src: Src,
    sid: String,
    off: u64,
}

fn roots() -> Vec<(PathBuf, Src)> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    vec![
        (home.join(".claude/projects"), Src::Claude),
        (home.join(".pi/agent/sessions"), Src::Pi),
    ]
}

fn collect(dir: &Path, src: Src, depth: usize, now: SystemTime, out: &mut Vec<(PathBuf, Src, u64)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if depth < MAX_DEPTH {
                collect(&p, src, depth + 1, now, out);
            }
        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let Ok(meta) = entry.metadata() else { continue };
            let recent = meta
                .modified()
                .ok()
                .and_then(|m| now.duration_since(m).ok())
                .map(|age| age <= RECENT)
                .unwrap_or(false);
            if recent {
                out.push((p, src, meta.len()));
            }
        }
    }
}

/// 扫描一次：新出现的文件从当前末尾开始跟踪，已跟踪但不再活跃的移除
fn scan(tracked: &mut HashMap<PathBuf, Tracked>) {
    let now = SystemTime::now();
    let mut found = Vec::new();
    for (root, src) in roots() {
        collect(&root, src, 0, now, &mut found);
    }
    for (path, src, len) in found {
        tracked.entry(path.clone()).or_insert_with(|| {
            let sid = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            Tracked { path, src, sid, off: len }
        });
    }
    tracked.retain(|_, t| {
        fs::metadata(&t.path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| now.duration_since(m).ok())
            .map(|age| age <= RECENT)
            .unwrap_or(false)
    });
}

/// 读取一个文件的新增完整行，返回解析出的记录（不缓存正文）
fn read_new_records(t: &mut Tracked) -> Vec<serde_json::Value> {
    let Ok(size) = fs::metadata(&t.path).map(|m| m.len()) else {
        return Vec::new();
    };
    if size < t.off {
        t.off = size; // 文件被截断：从新末尾继续
        return Vec::new();
    }
    if size == t.off {
        return Vec::new();
    }
    let want = (size - t.off).min(MAX_READ);
    let Ok(mut f) = File::open(&t.path) else {
        return Vec::new();
    };
    if f.seek(SeekFrom::Start(t.off)).is_err() {
        return Vec::new();
    }
    let mut buf = vec![0u8; want as usize];
    let Ok(n) = f.read(&mut buf) else {
        return Vec::new();
    };
    buf.truncate(n);

    let Some(end) = buf.iter().rposition(|&b| b == b'\n') else {
        // 没有换行：可能是正在写的半行，等下次；超过上限视为异常超长行，直接跳过
        if want == MAX_READ {
            t.off += want;
        }
        return Vec::new();
    };
    t.off += (end + 1) as u64;
    buf[..end]
        .split(|&b| b == b'\n')
        .filter(|l| !l.iter().all(|b| b.is_ascii_whitespace()))
        .filter_map(|l| serde_json::from_slice::<serde_json::Value>(l).ok())
        .collect()
}

fn shadow_only() -> bool {
    std::env::var("KB_TRANSCRIPT_SHADOW").map(|v| v == "1").unwrap_or(false)
}

fn watch_enabled() -> bool {
    std::env::var("KB_TRANSCRIPT_WATCH").map(|v| v != "0").unwrap_or(true)
}

pub fn start(app: AppHandle) {
    if !watch_enabled() {
        println!("[kapybara-buddy] 会话日志监听已关闭（KB_TRANSCRIPT_WATCH=0）");
        return;
    }
    let shadow = shadow_only();
    println!(
        "[kapybara-buddy] 会话日志监听启动（{}）",
        if shadow { "仅对照，不驱动桌宠" } else { "驱动桌宠" }
    );
    std::thread::spawn(move || {
        let mut tracked: HashMap<PathBuf, Tracked> = HashMap::new();
        let mut last_scan = std::time::Instant::now() - SCAN_EVERY;
        loop {
            if last_scan.elapsed() >= SCAN_EVERY {
                scan(&mut tracked);
                last_scan = std::time::Instant::now();
            }
            for t in tracked.values_mut() {
                for rec in read_new_records(t) {
                    let Some(phase) = classify(t.src, &rec) else { continue };
                    if shadow {
                        println!(
                            "[watch-shadow] [{}] 会话 {} -> {:?}",
                            t.src.name(),
                            truncate_id(&t.sid),
                            phase
                        );
                    } else {
                        agent_event::transcript_event(&app, t.src.name(), &t.sid, phase.ev_norm());
                    }
                }
            }
            std::thread::sleep(POLL);
        }
    });
}

fn truncate_id(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_tool_use_is_typing() {
        let r = json!({"type":"assistant","message":{"stop_reason":"tool_use","content":[{"type":"text","text":"x"},{"type":"tool_use","name":"Bash"}]}});
        assert_eq!(classify(Src::Claude, &r), Some(Phase::Typing));
    }

    #[test]
    fn claude_end_turn_is_settled() {
        let r = json!({"type":"assistant","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}});
        assert_eq!(classify(Src::Claude, &r), Some(Phase::Settled));
    }

    #[test]
    fn claude_user_prompt_is_thinking_and_tool_result_is_typing() {
        let prompt = json!({"type":"user","message":{"content":"hi"}});
        assert_eq!(classify(Src::Claude, &prompt), Some(Phase::Thinking));
        let result = json!({"type":"user","message":{"content":[{"type":"tool_result","content":"ok"}]}});
        assert_eq!(classify(Src::Claude, &result), Some(Phase::Typing));
    }

    #[test]
    fn claude_non_message_records_are_ignored() {
        let r = json!({"type":"permission-mode","permissionMode":"default"});
        assert_eq!(classify(Src::Claude, &r), None);
    }

    #[test]
    fn pi_stop_is_settled_and_tool_call_is_typing() {
        let stop = json!({"type":"message","message":{"role":"assistant","stopReason":"stop","content":[{"type":"text","text":"ok"}]}});
        assert_eq!(classify(Src::Pi, &stop), Some(Phase::Settled));
        let call = json!({"type":"message","message":{"role":"assistant","stopReason":"toolUse","content":[{"type":"toolCall","name":"read"}]}});
        assert_eq!(classify(Src::Pi, &call), Some(Phase::Typing));
        let tr = json!({"type":"message","message":{"role":"toolResult","content":[]}});
        assert_eq!(classify(Src::Pi, &tr), Some(Phase::Typing));
    }

    #[test]
    fn pi_non_message_records_are_ignored() {
        let r = json!({"type":"session","id":"x"});
        assert_eq!(classify(Src::Pi, &r), None);
    }

    #[test]
    fn partial_line_is_not_consumed() {
        let dir = std::env::temp_dir().join(format!("kb-watch-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("s.jsonl");
        fs::write(&p, b"{\"type\":\"user\",\"message\":{\"content\":\"a\"}}\n{\"type\":\"us").unwrap();
        let mut t = Tracked { path: p.clone(), src: Src::Claude, sid: "s".into(), off: 0 };
        let recs = read_new_records(&mut t);
        assert_eq!(recs.len(), 1, "只应读到完整的第一行");
        // 补全第二行后再读，应得到第二条
        let mut f = fs::OpenOptions::new().append(true).open(&p).unwrap();
        use std::io::Write;
        f.write_all(b"er\",\"message\":{\"content\":\"b\"}}\n").unwrap();
        let recs2 = read_new_records(&mut t);
        assert_eq!(recs2.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_offset_skips_history() {
        let dir = std::env::temp_dir().join(format!("kb-watch-hist-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("h.jsonl");
        fs::write(&p, b"{\"type\":\"user\",\"message\":{\"content\":\"old\"}}\n").unwrap();
        let len = fs::metadata(&p).unwrap().len();
        let mut t = Tracked { path: p.clone(), src: Src::Claude, sid: "h".into(), off: len };
        assert!(read_new_records(&mut t).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
