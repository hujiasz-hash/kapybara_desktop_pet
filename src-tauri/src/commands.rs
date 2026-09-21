//! IPC 命令（对应原 main.js 的 ipcMain handler + preload.js 暴露面）

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::answer;
use crate::drag;
use crate::state::AppState;

/// 提问：busy 拒绝、空问题拒绝，其余派发给后端
#[tauri::command]
pub fn ask(app: AppHandle, q: String) -> serde_json::Value {
    let question = q.trim().to_string();
    if question.is_empty() {
        return json!({ "ok": false, "reason": "empty" });
    }
    let st = app.state::<AppState>();
    match st
        .busy
        .compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst)
    {
        Err(_) => json!({ "ok": false, "reason": "busy" }),
        Ok(_) => {
            drop(st);
            answer::launch(&app, question);
            json!({ "ok": true })
        }
    }
}

#[tauri::command]
pub fn hide_chat(app: AppHandle) {
    if let Some(chat) = app.get_webview_window("chat") {
        let _ = chat.hide();
    }
}

#[tauri::command]
pub async fn stop_answer(app: AppHandle) -> Result<(), ()> {
    answer::stop(app).await;
    Ok(())
}

#[tauri::command]
pub fn pet_drag_start(app: AppHandle, sx: f64, sy: f64) {
    drag::on_drag_start(&app, sx, sy);
}

#[tauri::command]
pub fn pet_drag_move(app: AppHandle, sx: f64, sy: f64) {
    drag::on_drag_move(&app, sx, sy);
}

#[tauri::command]
pub fn pet_drag_end(app: AppHandle) {
    drag::on_drag_end(&app);
}

#[tauri::command]
pub fn pet_click(app: AppHandle) {
    crate::toggle_chat(&app);
}

/// 桌宠 WebView console 转发：写 pet-debug.log + 异常打到 stdout（对应原 console-message 捕获）
#[tauri::command]
pub fn pet_log(level: String, message: String) {
    let path = std::env::temp_dir().join("pet-debug.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = f.write_all(format!("[{}] {}\n", level, message).as_bytes());
    }
    if level == "warn" || level == "error" || message.contains("[pet]") {
        println!("[pet-console:{}] {}", level, message);
    }
}
