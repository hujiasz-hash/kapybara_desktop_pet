//! Capybara Buddy v5.0 — Tauri v2 实现
//!
//! - 桌面常驻卡皮巴拉（不占焦点、置顶、可拖动）
//! - 鼠标悬停桌宠 → 弹出订阅用量面板（Copilot / 智谱 / 自定义）
//! - 左键点击桌宠 → 弹出订阅配置面板（极简：智谱只填 Key；自定义 URL+Key）
//! - Option+G (mac) / Alt+G (Win) 呼出/隐藏配置面板
//! - pi / Antigravity / Claude Code 事件通道：127.0.0.1:17898（见 agent_event.rs）
//! - v4.x 的问答窗（pollinations/agy）已移除，见 git 历史

mod agent_event;
mod bridge;
mod commands;
mod drag;
mod geom;
mod pievent;
mod state;
mod usage;

use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use state::{AppState, Limit};

const HOTKEY_LABEL: &str = "Option+G (mac) / Alt+G (Win)";

fn main() {
    tauri::Builder::default()
        // 单实例：重复启动 → 主实例呼出配置面板（对应 requestSingleInstanceLock + second-instance）
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            toggle_config(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::pet_drag_start,
            commands::pet_drag_move,
            commands::pet_drag_end,
            commands::pet_click,
            commands::pet_log,
            commands::usage_state,
            commands::usage_save,
            commands::usage_refresh,
            commands::usage_panel_hover,
            commands::usage_panel_height,
            commands::hide_config,
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
            usage::start(handle.clone());

            // 全局快捷键（系统级，无需辅助功能授权）
            {
                let h2 = handle.clone();
                let registered = app
                    .global_shortcut()
                    .on_shortcut("alt+g", move |_app, _shortcut, event| {
                        if event.state == ShortcutState::Pressed {
                            toggle_config(&h2);
                        }
                    });
                if let Err(e) = registered {
                    println!(
                        "[kapybara-buddy] 快捷键 {} 注册失败（{}），可能被占用，请改 main.rs 里的热键",
                        HOTKEY_LABEL, e
                    );
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // 关闭一律转为隐藏（常驻后台）
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            // 配置面板失焦自动隐藏（点击面板外即收起）
            tauri::WindowEvent::Focused(false) => {
                if window.label() == "config" && window.is_visible().unwrap_or(false) {
                    let _ = window.hide();
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

    // ---- 订阅配置面板（预加载，左键点击/热键呼出即达） ----
    let config = WebviewWindowBuilder::new(app, "config", WebviewUrl::App("config.html".into()))
        .title("kapybara-config")
        .position(100.0, 100.0)
        .inner_size(geom::CONFIG_W, geom::CONFIG_H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .initialization_script(bridge::build_panel())
        .build()?;
    config.set_visible_on_all_workspaces(true)?;

    // ---- 悬浮用量面板（悬停桌宠弹出，不抢焦点） ----
    let usage_panel = WebviewWindowBuilder::new(app, "usage", WebviewUrl::App("usage.html".into()))
        .title("kapybara-usage")
        .position(100.0, 100.0)
        .inner_size(geom::USAGE_W, geom::USAGE_H)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .focusable(false) // 纯展示 + 刷新按钮，不抢焦点
        .accept_first_mouse(true)
        .visible(false)
        .initialization_script(bridge::build_panel())
        .build()?;
    usage_panel.set_visible_on_all_workspaces(true)?;

    prelearn(app.handle().clone(), expected_x_phys);
    Ok(())
}

/// 呼出/隐藏订阅配置面板：贴着宠物弹出，左边放不下放右侧（原 toggle_chat 语义）
pub fn toggle_config(app: &AppHandle) {
    let Some(config) = app.get_webview_window("config") else { return };
    let Some(pet) = app.get_webview_window("pet") else { return };
    if config.is_visible().unwrap_or(false) {
        let _ = config.hide();
        return;
    }
    usage::hide_now(app); // 用量面板让位

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

    let mut x = px - geom::CONFIG_W - 6.0; // 默认宠物左侧
    if x < wa.x + 4.0 {
        x = px + geom::PET_SIZE + 6.0; // 左边放不下放右侧
    }
    let y = (wa.y + 4.0).max(py - 60.0);
    let _ = config.set_position(geom::to_phys(x, y, ms));
    let _ = config.show();
    let _ = config.set_focus();
    // 显示瞬间推一次最新快照，免得等下一轮轮询
    let _ = app.emit("usage-update", usage::state_json(app));
}

/// 光标轮询：宠物朝向跟随（120ms）+ 悬停弹出用量面板 + 拖动泄漏兜底（对应主进程 setInterval）
fn start_cursor_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut hover = HoverState::default();
        let verbose = std::env::var("KB_USAGE_DEBUG").is_ok();
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

            hover_tick(&app, mx, my, &mut hover, verbose);

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
                    agent_event::pet_event(&app, "drag-lost", None);
                }
            }
        }
    });
}

/// 悬停判定参数：贴到桌宠身上满 350ms → 弹面板（沿用 pet.html 原先的防抖手感）
const HOVER_DELAY: Duration = Duration::from_millis(350);
/// 拖动中 / 松手后这段时间内不弹面板：拖动时窗口跟着光标走，光标必然压在桌宠上
const DRAG_MUTE: Duration = Duration::from_millis(800);

/// 悬停状态机（主进程侧判定，命中测试见 usage::cursor_in_hover_zone）
#[derive(Default)]
struct HoverState {
    /// 光标连续落在保持区内的起点（离开即清零，用于 350ms 防抖）
    inside_since: Option<Instant>,
    /// 面板当前是否由悬停维持显示（只在状态翻转时调一次 hover，避免每 tick 重复 IPC）
    shown: bool,
    /// 最近一次拖动活动时间（拖动中每 tick 刷新，用于松手后的静默期）
    last_drag_at: Option<Instant>,
}

/// 每 tick 判定一次悬停：进圈满 350ms 弹面板，出圈交给 usage::hover 的 600ms 宽限
/// （光标从桌宠挪到面板上的那几像素空隙不会被误判成"移开"）
///
/// `verbose`（KB_USAGE_DEBUG）逐 tick 打光标与桌宠矩形，排查"悬停哑火"用
fn hover_tick(app: &AppHandle, mx: f64, my: f64, hs: &mut HoverState, verbose: bool) {
    let st = app.state::<AppState>();
    if st.drag.lock().unwrap().is_some() {
        hs.last_drag_at = Some(Instant::now());
    }
    let muted = hs.last_drag_at.map(|t| t.elapsed() < DRAG_MUTE).unwrap_or(false);
    // 配置面板开着时让位：左键点桌宠 = 进配置，别在它脸上再弹一张用量面板
    let config_open = app
        .get_webview_window("config")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);

    let in_zone = usage::cursor_in_hover_zone(app, mx, my);
    if verbose {
        let rect = app.get_webview_window("pet").and_then(|p| {
            let pos = p.outer_position().ok()?;
            let s = geom::scale_at_phys(app, pos.x, pos.y);
            Some(format!(
                "pet 物理=({},{}) scale={} → CSS=({:.1},{:.1})+{}",
                pos.x,
                pos.y,
                s,
                pos.x as f64 / s,
                pos.y as f64 / s,
                geom::PET_SIZE
            ))
        });
        println!(
            "[usage] tick 光标=({:.0},{:.0}) zone={} muted={} config={} {}",
            mx,
            my,
            in_zone,
            muted,
            config_open,
            rect.unwrap_or_else(|| "pet 窗口缺失".into())
        );
    }

    if muted || config_open || !in_zone {
        hs.inside_since = None;
        if hs.shown {
            hs.shown = false;
            println!("[usage] 悬停离开 光标=({:.0}, {:.0})", mx, my);
            usage::hover(app, false);
        }
        return;
    }
    let since = *hs.inside_since.get_or_insert_with(Instant::now);
    if !hs.shown && since.elapsed() >= HOVER_DELAY {
        hs.shown = true;
        println!("[usage] 悬停命中 光标=({:.0}, {:.0}) 判定用时={}ms", mx, my, since.elapsed().as_millis());
        usage::hover(app, true);
    }
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
