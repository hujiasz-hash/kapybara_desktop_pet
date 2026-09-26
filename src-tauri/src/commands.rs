//! IPC 命令（v5.0：问答命令移除，新增订阅用量命令组）

use serde_json::json;
use tauri::{AppHandle, Manager};

use crate::drag;
use crate::state::AppState;
use crate::usage::{self, UsageConfig};

#[tauri::command]
pub fn pet_drag_start(app: AppHandle, sx: f64, sy: f64) {
    // 拖动开始：悬停面板立即隐藏，避免挡视线
    usage::hide_now(&app);
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

/// 左键点击桌宠：弹出订阅配置面板（原问答窗语义，v5.0 改）
#[tauri::command]
pub fn pet_click(app: AppHandle) {
    crate::toggle_config(&app);
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

// ---------- 订阅用量 ----------

/// 全量快照（配置 + 各账号用量 + 更新时间）
#[tauri::command]
pub fn usage_state(app: AppHandle) -> serde_json::Value {
    usage::state_json(&app)
}

/// 保存配置并立即刷新（config.html 的保存按钮）
#[tauri::command]
pub fn usage_save(app: AppHandle, config: serde_json::Value) -> serde_json::Value {
    let cfg: UsageConfig = match serde_json::from_value(config) {
        Ok(c) => c,
        Err(e) => return json!({ "ok": false, "reason": format!("配置格式错误: {}", e) }),
    };
    usage::save_config(&app, &cfg);
    {
        let core = &app.state::<AppState>().usage;
        core.lock().unwrap().config = cfg;
    }
    let h = app.clone();
    tauri::async_runtime::spawn(async move { usage::refresh_all(&h).await });
    json!({ "ok": true })
}

/// 手动刷新（悬浮面板的刷新按钮）
#[tauri::command]
pub fn usage_refresh(app: AppHandle) {
    let h = app.clone();
    tauri::async_runtime::spawn(async move { usage::refresh_all(&h).await });
}

/// 鼠标在面板内/外（面板窗口自己上报，用于保持/取消隐藏）
///
/// 弹面板本身由主进程按全局光标判定（main.rs hover_tick），桌宠窗不再上报悬停。
#[tauri::command]
pub fn usage_panel_hover(app: AppHandle, hovered: bool) {
    usage::hover(&app, hovered);
}

/// 用量面板上报自身内容高度（只显示已配置账号，卡片数不同高度不同）
#[tauri::command]
pub fn usage_panel_height(app: AppHandle, h: f64) {
    usage::set_panel_height(&app, h);
}

/// 配置面板 Esc / 手动关闭
#[tauri::command]
pub fn hide_config(app: AppHandle) {
    if let Some(w) = app.get_webview_window("config") {
        let _ = w.hide();
    }
}
