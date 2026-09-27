//! 拖动：统一桌面坐标 + 按显示器学习系统边界。

use std::time::Duration;
use std::time::Instant;

use tauri::{AppHandle, Manager, PhysicalPosition};

use crate::geom::{self, MonitorKey};
use crate::state::{AppState, DragState, Limit};

pub fn on_drag_start(app: &AppHandle, sx: f64, sy: f64) {
    let Some(pet) = app.get_webview_window("pet") else {
        return;
    };
    let Ok(pos) = pet.outer_position() else {
        return;
    };
    let (wx, wy) = geom::position_desktop(&pet, &pos);
    let monitor =
        geom::monitor_at_desktop(app, sx, sy).or_else(|| pet.current_monitor().ok().flatten());
    let monitor_key = monitor.as_ref().map(geom::monitor_key);
    let st = app.state::<AppState>();
    let mut d = st.drag.lock().unwrap();
    *d = Some(DragState {
        sx,
        sy,
        x: wx,
        y: wy,
        last_move: Instant::now(),
        expect: None,
        expect_monitor: None,
        stable_x: wx,
        stable_monitor: monitor_key,
    });
    // petLimit 进程级持久（启动预学 + 拖动中补学），这里不清空
}

pub fn on_drag_move(app: &AppHandle, sx: f64, sy: f64) {
    let st = app.state::<AppState>();
    let (sx0, sy0, wx, wy) = {
        let mut g = st.drag.lock().unwrap();
        match g.as_mut() {
            Some(d) => {
                d.last_move = Instant::now();
                (d.sx, d.sy, d.x, d.y)
            }
            None => return,
        }
    };

    // sx/sy、窗口位置和 workArea 均已转换到同一桌面坐标系；不在跨屏时重乘目标屏 scale。
    let Some(mon) = geom::monitor_at_desktop(app, sx, sy) else {
        return;
    };
    let monitor_key = geom::monitor_key(&mon);
    let wa = geom::work_area_desktop(&mon);
    let unit_scale = geom::logical_to_desktop(1.0, mon.scale_factor());
    let pet_size = geom::logical_to_desktop(geom::PET_SIZE, mon.scale_factor());
    let margin = 6.0 * unit_scale;
    let limit = limit_for_monitor(&st.pet_limit.lock().unwrap(), monitor_key)
        .unwrap_or_else(|| Limit::open(monitor_key));

    // 绝对坐标：按下时窗口位置 + 全局光标净位移。支持负坐标、不同缩放和任意屏幕排列。
    let (mut x, mut y) = desired_origin(wx, wy, sx0, sy0, sx, sy);
    let min_x = (wa.x + margin).max(limit.min_x);
    let max_x = (wa.x + wa.w - pet_size - margin).min(limit.max_x);
    let min_y = wa.y + margin;
    let max_y = wa.y + wa.h - pet_size - margin;
    x = clamp_origin(x, min_x, max_x);
    y = clamp_origin(y, min_y, max_y);
    let x = x.round();
    let y = y.round();

    if let Some(pet) = app.get_webview_window("pet") {
        let _ = geom::set_position_desktop(&pet, x, y);
    }
    {
        let mut g = st.drag.lock().unwrap();
        if let Some(d) = g.as_mut() {
            d.expect = Some((x, y));
            d.expect_monitor = Some(monitor_key);
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
}

/// 窗口位置变化：识别「系统弹回」（台前调度条），只在同一显示器内确认式学习真实边界。
pub fn on_pet_moved(app: &AppHandle, pos: &PhysicalPosition<i32>) {
    let Some(pet) = app.get_webview_window("pet") else {
        return;
    };
    let (actual_x, _) = geom::position_desktop(&pet, pos);
    let st = app.state::<AppState>();
    let expects = {
        let mut g = st.drag.lock().unwrap();
        match g.as_mut() {
            Some(d) => match (d.expect, d.expect_monitor) {
                (Some(expect), Some(monitor)) => {
                    if (actual_x - expect.0).abs() <= 3.0 {
                        // 位置符合期望：记录稳定点及其显示器，取消未决复核。
                        d.stable_x = actual_x;
                        d.stable_monitor = Some(monitor);
                        None
                    } else if d.stable_monitor == Some(monitor) {
                        Some((expect, d.stable_x, monitor))
                    } else {
                        // 刚跨屏时不拿上一块屏的落点学习新屏边界。
                        None
                    }
                }
                _ => None,
            },
            None => return,
        }
    };
    let Some((expect, stable_x, monitor)) = expects else {
        // 符合期望、没有同屏稳定点或拖动已结束 → 取消复核。
        let h = { st.move_confirm.lock().unwrap().take() };
        if let Some(h) = h {
            h.abort();
        }
        return;
    };
    // 位置不符：可能是系统弹回，也可能只是 setPosition 应用延迟——延时 150ms 复核再学。
    let mut mc = st.move_confirm.lock().unwrap();
    if mc.is_none() {
        let app2 = app.clone();
        *mc = Some(tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(150)).await;
            confirm_learn(&app2, expect, stable_x, monitor);
        }));
    }
}

fn confirm_learn(app: &AppHandle, expect: (f64, f64), stable_x: f64, monitor: MonitorKey) {
    let st = app.state::<AppState>();
    let snapshot = {
        let g = st.drag.lock().unwrap();
        g.as_ref().and_then(|d| {
            if d.expect == Some(expect)
                && d.expect_monitor == Some(monitor)
                && d.stable_monitor == Some(monitor)
            {
                Some(())
            } else {
                None
            }
        })
    };
    if snapshot.is_none() {
        st.move_confirm.lock().unwrap().take();
        return;
    }
    let Some(pet) = app.get_webview_window("pet") else {
        st.move_confirm.lock().unwrap().take();
        return;
    };
    let Ok(actual) = pet.outer_position() else {
        st.move_confirm.lock().unwrap().take();
        return;
    };
    let (actual_x, _) = geom::position_desktop(&pet, &actual);
    if (actual_x - expect.0).abs() > 3.0 {
        // 确认真弹回（系统持续拒绝期望位置）：真实边界在最后一次同屏稳定位置附近。
        let dir = if expect.0 > actual_x { 1 } else { -1 };
        let mut limits = st.pet_limit.lock().unwrap();
        let mut limit = limit_for_monitor(&limits, monitor).unwrap_or_else(|| Limit::open(monitor));
        let candidate = if dir > 0 {
            Limit {
                monitor,
                min_x: limit.min_x,
                max_x: stable_x,
            }
        } else {
            Limit {
                monitor,
                min_x: stable_x,
                max_x: limit.max_x,
            }
        };
        // 防钉死：边界必须留出足够活动空间。
        if candidate.min_x < candidate.max_x - 200.0 {
            limit = candidate;
            upsert_limit(&mut limits, limit);
            println!(
                "[kapybara-buddy] 拖动边界学习: monitor={:?} minX={:.0} maxX={:.0}",
                monitor, limit.min_x, limit.max_x
            );
        }
        if let Some(d) = st.drag.lock().unwrap().as_mut() {
            if d.expect == Some(expect) && d.expect_monitor == Some(monitor) {
                d.expect = None;
                d.expect_monitor = None;
            }
        }
    }
    // 已追平 → 只是延迟，不学。
    st.move_confirm.lock().unwrap().take();
}

/// 启动预学：初始位置若被系统弹回，用对应屏幕的统一坐标学习边界。
pub fn prelearn(app: AppHandle, expected_x: f64, monitor: MonitorKey) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let Some(pet) = app.get_webview_window("pet") else {
            return;
        };
        if let Ok(position) = pet.outer_position() {
            let (actual_x, _) = geom::position_desktop(&pet, &position);
            if actual_x < expected_x - 5.0 {
                let state = app.state::<AppState>();
                let mut limits = state.pet_limit.lock().unwrap();
                upsert_limit(
                    &mut limits,
                    Limit {
                        monitor,
                        min_x: f64::NEG_INFINITY,
                        max_x: actual_x,
                    },
                );
                println!(
                    "[kapybara-buddy] 台前调度右边界预学: monitor={:?} maxX={:.0}",
                    monitor, actual_x
                );
            }
        }
    });
}

fn limit_for_monitor(limits: &[Limit], monitor: MonitorKey) -> Option<Limit> {
    limits.iter().find(|limit| limit.monitor == monitor).copied()
}

fn upsert_limit(limits: &mut Vec<Limit>, new_limit: Limit) {
    if let Some(existing) = limits
        .iter_mut()
        .find(|limit| limit.monitor == new_limit.monitor)
    {
        *existing = new_limit;
    } else {
        limits.push(new_limit);
    }
}

fn desired_origin(
    window_x: f64,
    window_y: f64,
    start_mouse_x: f64,
    start_mouse_y: f64,
    mouse_x: f64,
    mouse_y: f64,
) -> (f64, f64) {
    (
        window_x + mouse_x - start_mouse_x,
        window_y + mouse_y - start_mouse_y,
    )
}

fn clamp_origin(value: f64, min: f64, max: f64) -> f64 {
    if min <= max {
        value.clamp(min, max)
    } else {
        // 屏幕比窗口还小的极端情况：至少让窗口左上角留在工作区起点附近。
        min
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(x: i32, scale_milli: u32) -> MonitorKey {
        MonitorKey {
            x,
            y: 0,
            width: 1920,
            height: 1080,
            scale_milli,
        }
    }

    #[test]
    fn pointer_delta_stays_continuous_across_different_display_scales() {
        // 光标从 2x 屏幕进入 1x 屏幕后，整个桌面坐标系仍是同一单位，位移不重新缩放。
        let origin = desired_origin(1400.0, 120.0, 1450.0, 170.0, 1650.0, 270.0);
        assert_eq!(origin, (1600.0, 220.0));
    }

    #[test]
    fn learned_boundary_is_only_applied_to_its_own_monitor() {
        let primary = screen(0, 2000);
        let external = screen(3024, 1000);
        let limits = vec![Limit {
            monitor: primary,
            min_x: f64::NEG_INFINITY,
            max_x: 1480.0,
        }];

        assert_eq!(limit_for_monitor(&limits, primary).unwrap().max_x, 1480.0);
        assert!(limit_for_monitor(&limits, external).is_none());
    }

    #[test]
    fn clamp_handles_negative_origin_and_too_small_work_area() {
        assert_eq!(clamp_origin(-2100.0, -1900.0, -100.0), -1900.0);
        assert_eq!(clamp_origin(0.0, 40.0, 20.0), 40.0);
    }
}
