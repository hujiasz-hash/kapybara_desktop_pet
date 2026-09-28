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

use state::AppState;

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
    let expected_x = geom::logical_to_desktop(pet_x, s);

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

    drag::prelearn(app.handle().clone(), expected_x, geom::monitor_key(&mon));
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

    let Some(pet_geo) = geom::window_geometry_desktop(&pet) else { return };
    let px = pet_geo.rect.x;
    let py = pet_geo.rect.y;
    let Some(mon) = geom::monitor_at_desktop(
        app,
        px + pet_geo.rect.w / 2.0,
        py + pet_geo.rect.h / 2.0,
    ) else {
        return;
    };
    let wa = geom::work_area_desktop(&mon);
    let unit = geom::logical_to_desktop(1.0, mon.scale_factor());
    let config_w = geom::logical_to_desktop(geom::CONFIG_W, mon.scale_factor());

    let mut x = px - config_w - 6.0 * unit; // 默认宠物左侧
    if x < wa.x + 4.0 * unit {
        x = px + pet_geo.rect.w + 6.0 * unit; // 左边放不下放右侧
    }
    let y = (wa.y + 4.0 * unit).max(py - 60.0 * unit);
    let _ = geom::set_position_desktop(&config, x, y);
    let _ = config.show();
    let _ = config.set_focus();
    // 显示瞬间推一次最新快照，免得等下一轮轮询
    let _ = app.emit("usage-update", usage::state_json(app));
}

/// 光标轮询节奏（v5.5）
/// - 快拍 120ms：朝向跟随 / 悬停 350ms 防抖 / 拖动 0.9s 泄漏兜底所需的时间分辨率
/// - 慢拍 500ms：光标停住且离桌宠远——只为发现"鼠标又动了"。
///   此前这里不分场合恒定 120ms：夜间无人操作也每秒 8 次往 WKWebView 灌 JS
///   （emit_to 即一次 runJavaScriptInFrameInScriptWorld，且每次都取一回
///   WebContent 的 foreground activity 断言，WebKit 整夜无法挂起、unified log
///   每分钟刷 2000 条）
const TICK_FAST: Duration = Duration::from_millis(120);
const TICK_IDLE: Duration = Duration::from_millis(500);
/// 光标停住后快拍的保持期：短暂停顿再动不应掉进慢拍
const ACTIVITY_GRACE: Duration = Duration::from_secs(2);

/// 推给渲染层的光标快照：光标位置 + 桌宠窗口矩形 + 缩放。
/// 渲染层收到重复载荷是纯 no-op（全部按绝对坐标幂等重算），所以主进程只在
/// 快照变化时才 emit——静止的光标不该把 WebKit 反复拽起来跑 JS。
#[derive(Clone, Copy, Debug, PartialEq)]
struct CursorSnapshot {
    mx: f64,
    my: f64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    scale: f64,
}

impl CursorSnapshot {
    /// 是否值得再推一帧：任一字段变化超过半像素（亚像素抖动不算），或缩放变化。
    fn diff(&self, o: &CursorSnapshot) -> bool {
        const EPS: f64 = 0.5;
        (self.mx - o.mx).abs() > EPS
            || (self.my - o.my).abs() > EPS
            || (self.x - o.x).abs() > EPS
            || (self.y - o.y).abs() > EPS
            || (self.w - o.w).abs() > EPS
            || (self.h - o.h).abs() > EPS
            || (self.scale - o.scale).abs() > 1e-6
    }
}

/// 光标轮询：宠物朝向跟随 + 悬停弹出用量面板 + 拖动泄漏兜底
///
/// 两层收敛（v5.5）：
/// 1. 快照没变不推送——光标、桌宠矩形、缩放与上次完全一致时跳过 emit；
/// 2. 节奏自适应——拖动中 / 光标在悬停区 / 用量面板可见 / 光标近期动过 → 快拍；
///    其余（鼠标停着且离桌宠远）→ 慢拍。整夜静止时对外零 JS 注入，
///    主进程也只剩每秒 2 次本地光标读取，WebKit 可以真正挂起。
fn start_cursor_loop(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut hover = HoverState::default();
        let verbose = std::env::var("KB_USAGE_DEBUG").is_ok();
        let mut last_sent: Option<CursorSnapshot> = None; // 上次真正 emit 的快照
        let mut last_cursor: Option<(f64, f64)> = None; // 上次读到的光标（判定"动过"）
        let mut last_move_at = Instant::now();
        let mut last_idle_log = Instant::now();
        let mut fast = true; // 起步按快拍跑，首拍即把初始状态喂给渲染层
        loop {
            tokio::time::sleep(if fast { TICK_FAST } else { TICK_IDLE }).await;
            let Some(pet) = app.get_webview_window("pet") else { fast = false; continue };
            let Some((mx, my)) = geom::cursor_desktop(&app) else { fast = false; continue };
            let Some(pet_geo) = geom::window_geometry_desktop(&pet) else { fast = true; continue };
            let rect = pet_geo.rect;
            let pet_visible = pet.is_visible().unwrap_or(true);

            // 光标位移超过半像素才算"动过"（渲染层同样按位移唤醒睡意，口径一致）
            let moved = last_cursor
                .map(|(lx, ly)| (mx - lx).abs() > 0.5 || (my - ly).abs() > 0.5)
                .unwrap_or(true);
            if moved {
                last_move_at = Instant::now();
            }
            last_cursor = Some((mx, my));

            // 快照变化才推；桌宠窗口被隐藏时也不必喂光标
            let snap = CursorSnapshot {
                mx,
                my,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
                scale: pet_geo.logical_scale,
            };
            let changed = last_sent.map(|s| s.diff(&snap)).unwrap_or(true);
            if changed && pet_visible {
                last_sent = Some(snap);
                let _ = app.emit_to(
                    "pet",
                    "cursor",
                    serde_json::json!({
                        "mx": mx, "my": my,
                        "px": rect.x, "py": rect.y,
                        "pw": rect.w, "ph": rect.h,
                        "unitScale": pet_geo.logical_scale
                    }),
                );
            }

            // Debug 日志纪律：快拍逐条打，慢拍最多 2s 一条心跳
            // （v5.1 排查悬停哑火时曾整夜开着 KB_USAGE_DEBUG，一晚刷出 31 万行 tick）
            let log_tick = verbose
                && (fast || {
                    let due = last_idle_log.elapsed() >= Duration::from_secs(2);
                    if due {
                        last_idle_log = Instant::now();
                    }
                    due
                });
            let in_zone = hover_tick(&app, mx, my, &mut hover, log_tick);

            // 兜底：拖动 IPC 停滞 0.9s 且光标在窗口外（外扩 50px）→ mouseup 丢失，强制结束拖动
            let st = app.state::<AppState>();
            let drag_active = st.drag.lock().unwrap().is_some();
            if drag_active {
                let stale = {
                    let g = st.drag.lock().unwrap();
                    g.as_ref()
                        .map(|d| d.last_move.elapsed() > Duration::from_millis(900))
                        .unwrap_or(false)
                };
                if stale {
                    let pad = 50.0 * pet_geo.logical_scale;
                    let outside = mx < rect.x - pad
                        || mx > rect.x + rect.w + pad
                        || my < rect.y - pad
                        || my > rect.y + rect.h + pad;
                    if outside {
                        *st.drag.lock().unwrap() = None;
                        agent_event::pet_event(&app, "drag-lost", None);
                    }
                }
            }

            // 下一拍节奏：有活干快拍，彻底闲置慢拍（隐藏的桌宠不需要跟光标）
            let panel_visible = app
                .get_webview_window("usage")
                .map(|w| w.is_visible().unwrap_or(false))
                .unwrap_or(false);
            fast = drag_active
                || in_zone
                || panel_visible
                || (pet_visible && last_move_at.elapsed() < ACTIVITY_GRACE);
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
/// 返回光标是否在悬停区内（主循环用它决定下一拍节奏）。
/// `log_tick`（KB_USAGE_DEBUG）打光标与桌宠矩形，排查"悬停哑火"用——
/// 只在快拍或慢拍心跳时打，静止时不再逐 tick 刷日志
fn hover_tick(app: &AppHandle, mx: f64, my: f64, hs: &mut HoverState, log_tick: bool) -> bool {
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
    if log_tick {
        let rect = app.get_webview_window("pet").and_then(|p| {
            let geo = geom::window_geometry_desktop(&p)?;
            Some(format!(
                "pet 桌面=({:.0},{:.0})+{:.0}x{:.0} unitScale={:.2}",
                geo.rect.x, geo.rect.y, geo.rect.w, geo.rect.h, geo.logical_scale
            ))
        });
        println!(
            "[usage] tick 桌面光标=({:.0},{:.0}) zone={} muted={} config={} {}",
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
        return in_zone;
    }
    let since = *hs.inside_since.get_or_insert_with(Instant::now);
    if !hs.shown && since.elapsed() >= HOVER_DELAY {
        hs.shown = true;
        println!("[usage] 悬停命中 光标=({:.0}, {:.0}) 判定用时={}ms", mx, my, since.elapsed().as_millis());
        usage::hover(app, true);
    }
    in_zone
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(mx: f64, my: f64, x: f64, y: f64, w: f64, h: f64, scale: f64) -> CursorSnapshot {
        CursorSnapshot { mx, my, x, y, w, h, scale }
    }

    #[test]
    fn identical_snapshots_are_not_re_emitted() {
        let s = snap(100.0, 200.0, 10.0, 20.0, 110.0, 110.0, 1.0);
        assert!(!s.diff(&s));
    }

    #[test]
    fn subpixel_jitter_is_not_a_change() {
        // 光标静止时 CGEvent 读数与窗口矩形不该有亚像素漂移，但万一有也不当变化
        let a = snap(100.0, 200.0, 10.0, 20.0, 110.0, 110.0, 1.0);
        let b = snap(100.3, 200.2, 10.1, 20.4, 110.2, 110.0, 1.0);
        assert!(!a.diff(&b));
    }

    #[test]
    fn real_changes_are_re_emitted() {
        let a = snap(100.0, 200.0, 10.0, 20.0, 110.0, 110.0, 1.0);
        // 光标动了 1px（渲染层要跟着转头/唤醒）
        assert!(a.diff(&snap(101.0, 200.0, 10.0, 20.0, 110.0, 110.0, 1.0)));
        // 桌宠被拖到别处（相对位置全变）
        assert!(a.diff(&snap(100.0, 200.0, 12.0, 25.0, 110.0, 110.0, 1.0)));
        // 跨屏缩放变化（朝向阈值按 unitScale 换算）
        assert!(a.diff(&snap(100.0, 200.0, 10.0, 20.0, 110.0, 110.0, 1.25)));
    }
}
