//! Capybara Buddy v4.0 — Tauri v2 实现（去 Electron）
//!
//! - 桌面常驻卡皮巴拉（不占焦点、置顶、可拖动），点击它弹出问答窗
//! - Option+G (mac) / Alt+G (Win) 呼出/隐藏问答窗
//! - 后端：pollinations（默认，零 key 免费网关）/ agy（KB_BACKEND=agy，流式）
//! - pi 扩展事件通道：127.0.0.1:17898（见 pievent.rs）

mod agent_event;
mod answer;
mod bridge;
mod commands;
mod drag;
mod geom;
mod pievent;
mod state;

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use state::{AppState, Limit};

const HOTKEY_LABEL: &str = "Option+G (mac) / Alt+G (Win)";

fn main() {
    tauri::Builder::default()
        // 单实例：重复启动 → 主实例呼出问答窗（对应 requestSingleInstanceLock + second-instance）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            toggle_chat(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::ask,
            commands::hide_chat,
            commands::stop_answer,
            commands::pet_drag_start,
            commands::pet_drag_move,
            commands::pet_drag_end,
            commands::pet_click,
            commands::pet_log,
        ])
        .setup(|app| {
            // 不进 Dock（对应 app.dock.hide()）
            #[cfg(target_os = "macos")]
            {
                let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }
            let handle = app.handle().clone();

            create_windows(app)?;
            start_cursor_loop(handle.clone());
            agent_event::start(handle.clone());
            println!("[kapybara-buddy] 后端: {}", answer::backend_desc());

            // 全局快捷键（系统级，无需辅助功能授权）
            {
                let h2 = handle.clone();
                let registered = app
                    .global_shortcut()
                    .on_shortcut("alt+g", move |_app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            toggle_chat(&h2);
                        }
                    });
                if let Err(e) = registered {
                    println!(
                        "[kapybara-buddy] 快捷键 {} 注册失败（{}），可能被占用，请改 main.rs 里的热键",
                        HOTKEY_LABEL, e
                    );
                }
            }

            // 自动化测试：KB_TEST_ASK="问题"（KB_TEST_ASK2 第二问验证覆盖）
            test_flow(handle);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // 关闭一律转为隐藏（常驻后台，对应 window-all-closed no-op + chat closed 重建语义）
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            // 问答窗失焦自动隐藏（回答生成中除外）
            tauri::WindowEvent::Focused(false) => {
                if window.label() == "chat" {
                    let app = window.app_handle();
                    let busy = app.state::<AppState>().busy.load(Ordering::SeqCst);
                    if !busy && window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    }
                }
            }
            // 拖动中的系统弹回识别与边界学习
            tauri::WindowEvent::Moved(pos) => {
                if window.label() == "pet" {
                    let app = window.app_handle().clone();
                    drag::on_pet_moved(&app, pos);
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running kapybara-buddy");
}

fn create_windows(app: &tauri::App) -> tauri::Result<()> {
    // ---- 桌宠窗（常驻，不抢焦点） ----
    let mon = app.primary_monitor().ok().flatten().expect("no primary monitor");
    let s = mon.scale_factor();
    let wa = mon.work_area();
    let pet_x = wa.position.x as f64 / s + wa.size.width as f64 / s - 130.0;
    let pet_y = wa.position.y as f64 / s + 70.0;
    let expected_x_phys = (pet_x * s).round() as i32;

    let pet = WebviewWindowBuilder::new(app, "pet", WebviewUrl::App("pet.html".into()))
        .title("kapybara-pet")
        .position(pet_x, pet_y)
        .inner_size(geom::PET_SIZE, geom::PET_SIZE)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false) // 点击不抢当前应用焦点
        .accept_first_mouse(true)
        .visible(false)
        .initialization_script(bridge::build_pet())
        .build()?;
    pet.set_visible_on_all_workspaces(true)?; // 全 Space 可见（含全屏）
    let _ = pet.show(); // 非焦点窗口：show 不夺焦点（对应 showInactive）
    println!(
        "[kapybara-buddy] pet 初始=({:.0}, {:.0}) workArea={}x{}+{}+{}",
        pet_x,
        pet_y,
        wa.size.width as f64 / s,
        wa.size.height as f64 / s,
        wa.position.x,
        wa.position.y
    );

    // ---- 问答窗（预加载，呼出即达） ----
    let chat = WebviewWindowBuilder::new(app, "chat", WebviewUrl::App("index.html".into()))
        .title("kapybara-chat")
        .position(100.0, 100.0)
        .inner_size(geom::CHAT_W, geom::CHAT_H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .initialization_script(bridge::build_chat())
        .build()?;
    chat.set_visible_on_all_workspaces(true)?;

    prelearn(app.handle().clone(), expected_x_phys);
    Ok(())
}

/// 呼出/隐藏问答窗：贴着宠物左侧弹出，左边放不下放右侧（对应 toggleChat）
pub fn toggle_chat(app: &AppHandle) {
    let Some(chat) = app.get_webview_window("chat") else { return };
    let Some(pet) = app.get_webview_window("pet") else { return };
    if chat.is_visible().unwrap_or(false) {
        let _ = chat.hide();
        return;
    }
    let Ok(pos) = pet.outer_position() else { return };
    let ps = geom::scale_at_phys(app, pos.x, pos.y);
    let px = pos.x as f64 / ps;
    let py = pos.y as f64 / ps;

    let Some(mon) = geom::monitor_at_css(app, px + 55.0, py + 55.0)
        .or_else(|| app.primary_monitor().ok().flatten())
    else {
        return;
    };
    let wa = geom::work_area_css(&mon);
    let ms = mon.scale_factor();

    let mut x = px - geom::CHAT_W - 6.0; // 默认宠物左侧
    if x < wa.x + 4.0 {
        x = px + geom::PET_SIZE + 6.0; // 左边放不下放右侧
    }
    let y = (wa.y + 4.0).max(py - 60.0);
    let _ = chat.set_position(geom::to_phys(x, y, ms));
    let _ = chat.show();
    let _ = chat.set_focus();
    let _ = app.emit_to("chat", "focus-input", ());
    answer::pet_event(app, "heart", None); // 打开问答，捧爱心迎接
}

/// 光标轮询：宠物朝向跟随（120ms）+ 拖动泄漏兜底（对应主进程 setInterval）
fn start_cursor_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(120)).await;
            let Some(pet) = app.get_webview_window("pet") else { continue };
            let (mx, my) = geom::cursor_css(&app);
            let Ok(pos) = pet.outer_position() else { continue };
            let s = geom::scale_at_phys(&app, pos.x, pos.y);
            let px = pos.x as f64 / s;
            let py = pos.y as f64 / s;
            let _ = app.emit_to(
                "pet",
                "cursor",
                serde_json::json!({ "mx": mx, "my": my, "px": px, "py": py }),
            );

            // 兜底：拖动 IPC 停滞 0.9s 且光标在窗口外（外扩 50px）→ mouseup 丢失，强制结束拖动
            let st = app.state::<AppState>();
            let stale = {
                let g = st.drag.lock().unwrap();
                g.as_ref()
                    .map(|d| d.last_move.elapsed() > Duration::from_millis(900))
                    .unwrap_or(false)
            };
            if stale {
                let outside = mx < px - 50.0 || mx > px + 160.0 || my < py - 50.0 || my > py + 160.0;
                if outside {
                    *st.drag.lock().unwrap() = None;
                    answer::pet_event(&app, "drag-lost", None);
                }
            }
        }
    });
}

/// 启动预学：初始位置若被系统弹回（台前调度条），用弹回结果学边界（README v2.4）
fn prelearn(app: AppHandle, expected_x_phys: i32) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let Some(pet) = app.get_webview_window("pet") else { return };
        if let Ok(p) = pet.outer_position() {
            if p.x < expected_x_phys - 5 {
                *app.state::<AppState>().pet_limit.lock().unwrap() =
                    Some(Limit { min_x: i32::MIN / 2, max_x: p.x });
                println!("[kapybara-buddy] 台前调度右边界预学: maxX={}", p.x);
            }
        }
    });
}

/// 自动化测试：KB_TEST_ASK 提交一问（KB_TEST_ASK2 第二问验证覆盖），回答完成后打印并退出
fn test_flow(app: AppHandle) {
    let Ok(q1) = std::env::var("KB_TEST_ASK") else { return };
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(2500)).await;
        toggle_chat(&app);
        tokio::time::sleep(Duration::from_millis(800)).await;
        eval_submit(&app, &q1);
        let q2 = std::env::var("KB_TEST_ASK2").ok();
        let mut target = 1u32;
        if let Some(q2) = q2 {
            for _ in 0..60 {
                if state::test_done_count(&app) >= 1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            eval_submit(&app, &q2);
            target = 2;
        }
        for _ in 0..120 {
            if state::test_done_count(&app) >= target {
                break;
            }
            tokio::time::sleep(Duration::from_millis(1000)).await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        let text = app.state::<AppState>().test_buf.lock().unwrap().clone();
        let tail: String = {
            let chars: Vec<char> = text.chars().collect();
            let start = chars.len().saturating_sub(1200);
            chars[start..].iter().collect()
        };
        println!("---- 回答 ----\n{}", tail);
        app.exit(0);
    });
}

fn eval_submit(app: &AppHandle, q: &str) {
    if let Some(chat) = app.get_webview_window("chat") {
        let js = format!(
            "document.getElementById('q').value = {}; submit(); true;",
            serde_json::to_string(q).unwrap_or_else(|_| "''".into())
        );
        let _ = chat.eval(js);
    }
}
