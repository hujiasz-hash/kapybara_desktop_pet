//! 拖动：绝对坐标 + 台前调度边界学习（移植自 main.js 的 pet-drag-* 与 petWin.on('move')）

use std::time::Duration;
use std::time::Instant;

use tauri::{AppHandle, Manager, PhysicalPosition};

use crate::geom;
use crate::state::{AppState, DragState, Limit};

pub fn on_drag_start(app: &AppHandle, sx: f64, sy: f64) {
    let Some(pet) = app.get_webview_window("pet") else { return };
    let Ok(pos) = pet.outer_position() else { return };
    let st = app.state::<AppState>();
    let mut d = st.drag.lock().unwrap();
    *d = Some(DragState {
        sx,
        sy,
        x: pos.x,
        y: pos.y,
        last_move: Instant::now(),
        expect: None,
        stable_x: pos.x,
    });
    // petLimit 进程级持久（启动预学 + 拖动中补学），这里不清空
}

pub fn on_drag_move(app: &AppHandle, sx: f64, sy: f64) {
    let st = app.state::<AppState>();
    let (sx0, sy0, wx, wy) = {
        let g = st.drag.lock().unwrap();
        match g.as_ref() {
            Some(d) => (d.sx, d.sy, d.x, d.y),
            None => return,
        }
    };
    {
        let mut g = st.drag.lock().unwrap();
        if let Some(d) = g.as_mut() {
            d.last_move = Instant::now();
        }
    }

    // 鼠标所在显示器的工作区与 scale
    let Some(mon) = geom::monitor_at_css(app, sx, sy).or_else(|| app.primary_monitor().ok().flatten()) else {
        return;
    };
    let wa = mon.work_area();
    let s = mon.scale_factor();

    // 绝对坐标：按下时窗口位置 + 鼠标净位移（CSS→物理乘 scale），边缘 clamp 不累积漂移
    let mut x = wx as f64 + (sx - sx0) * s;
    let mut y = wy as f64 + (sy - sy0) * s;

    let pet_s = geom::PET_SIZE * s;
    let m = 6.0 * s;
    let lim = *st.pet_limit.lock().unwrap();
    let limit = lim.unwrap_or_else(Limit::open);

    // 学习到的系统真实边界优先（比 workArea 更紧）
    let min_x = (wa.position.x as f64 + m).max(limit.min_x as f64);
    let max_x = (wa.position.x as f64 + wa.size.width as f64 - pet_s - m).min(limit.max_x as f64);
    x = x.clamp(min_x, max_x);
    y = y.clamp(
        wa.position.y as f64 + m,
        wa.position.y as f64 + wa.size.height as f64 - pet_s - m,
    );
    let xi = x.round() as i32;
    let yi = y.round() as i32;

    if let Some(pet) = app.get_webview_window("pet") {
        let _ = pet.set_position(PhysicalPosition::new(xi, yi));
    }
    {
        let mut g = st.drag.lock().unwrap();
        if let Some(d) = g.as_mut() {
            d.expect = Some((xi, yi));
        }
    }
}

pub fn on_drag_end(app: &AppHandle) {
    let st = app.state::<AppState>();
    *st.drag.lock().unwrap() = None;
    let handle = { st.move_confirm.lock().unwrap().take() };
    if let Some(h) = handle {
        h.abort();
    }
    // 拖动中的多次 setPosition 会让 WKWebView 在窗口矩形内画出灰色背景
    // （透明性失效）。拖动结束重新声明透明背景：wry 的 set_background_color 在
    // macOS 上会重设 drawsBackground=false（与 transparent 特性同一个 KVC 键）。
    if let Some(pet) = app.get_webview_window("pet") {
        let _ = pet.set_background_color(Some(tauri::window::Color(0, 0, 0, 0)));
    }
}

/// 窗口位置变化：识别「系统弹回」（台前调度条），确认式学习真实边界（README v2.2~v2.4）
pub fn on_pet_moved(app: &AppHandle, pos: &PhysicalPosition<i32>) {
    let st = app.state::<AppState>();
    let expects = {
        let mut g = st.drag.lock().unwrap();
        match g.as_mut() {
            Some(d) => match d.expect {
                Some(expect) => {
                    if (pos.x - expect.0).abs() <= 3 {
                        // 位置符合期望：记录稳定点，取消未决的复核
                        d.stable_x = pos.x;
                        None
                    } else {
                        Some((expect, d.stable_x))
                    }
                }
                None => None,
            },
            None => return,
        }
    };
    let Some((_expect, _stable_x)) = expects else {
        // 符合期望 → 取消复核任务
        let h = { st.move_confirm.lock().unwrap().take() };
        if let Some(h) = h {
            h.abort();
        }
        return;
    };
    // 位置不符：可能是系统弹回，也可能只是 setPosition 应用延迟——延时 150ms 复核再学
    let mut mc = st.move_confirm.lock().unwrap();
    if mc.is_none() {
        let app2 = app.clone();
        *mc = Some(tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            confirm_learn(&app2);
        }));
    }
}

fn confirm_learn(app: &AppHandle) {
    let st = app.state::<AppState>();
    let snapshot = {
        let g = st.drag.lock().unwrap();
        match g.as_ref() {
            Some(d) => d.expect.map(|e| (e, d.stable_x)),
            None => None,
        }
    };
    let Some((expect, stable_x)) = snapshot else {
        st.move_confirm.lock().unwrap().take();
        return;
    };
    let Some(pet) = app.get_webview_window("pet") else { return };
    let Ok(actual) = pet.outer_position() else {
        st.move_confirm.lock().unwrap().take();
        return;
    };
    if (actual.x - expect.0).abs() > 3 {
        // 确认真弹回（系统持续拒绝期望位置）：真实边界在最后一次稳定位置附近
        let dir: i32 = if expect.0 > actual.x { 1 } else { -1 };
        let mut lim = st.pet_limit.lock().unwrap().unwrap_or_else(Limit::open);
        let cand = if dir > 0 {
            Limit { min_x: lim.min_x, max_x: stable_x }
        } else {
            Limit { min_x: stable_x, max_x: lim.max_x }
        };
        // 防钉死：边界必须留出足够活动空间
        if cand.min_x < cand.max_x - 200 {
            lim = cand;
            *st.pet_limit.lock().unwrap() = Some(lim);
            println!("[kapybara-buddy] 拖动边界学习: minX={} maxX={}", lim.min_x, lim.max_x);
        }
        if let Some(d) = st.drag.lock().unwrap().as_mut() {
            d.expect = None;
        }
    }
    // 已追平 → 只是延迟，不学
    st.move_confirm.lock().unwrap().take();
}
