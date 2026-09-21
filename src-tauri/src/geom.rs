//! 坐标换算工具
//!
//! 坐标模型（已对照 tao/tauri 源码确认）：
//! - Tauri/tao 的「物理坐标」= 屏幕点坐标（CSS px）× scale_factor（macOS Retina 上 scale=2）
//! - WKWebView 的 e.screenX/screenY 与 CGEventGetLocation 同为「点」坐标 → 即逻辑坐标
//! - monitor_from_point(macOS) 吃的是全局「点」坐标；Windows 分支按主屏 scale 近似换算
//! - tao 的 cursor_position 在 macOS Retina 有 bug（用 backing 像素做 Y 翻转），
//!   因此这里直接走 CoreGraphics 取光标位置

use tauri::{AppHandle, Monitor};

pub const PET_SIZE: f64 = 110.0;
pub const CHAT_W: f64 = 640.0;
pub const CHAT_H: f64 = 400.0;

/// 全局光标位置（CSS px，屏幕左上角为原点）
#[cfg(target_os = "macos")]
pub fn cursor_css(_app: &AppHandle) -> (f64, f64) {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    extern "C" {
        fn CGEventCreate(source: *const std::ffi::c_void) -> *mut std::ffi::c_void;
        fn CGEventGetLocation(event: *mut std::ffi::c_void) -> CGPoint;
        fn CFRelease(cf: *mut std::ffi::c_void);
    }
    unsafe {
        let ev = CGEventCreate(std::ptr::null());
        if ev.is_null() {
            return (0.0, 0.0);
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        (p.x, p.y) // 已是左上角原点的全局点坐标
    }
}

#[cfg(not(target_os = "macos"))]
pub fn cursor_css(app: &AppHandle) -> (f64, f64) {
    let s = primary_scale(app);
    match app.cursor_position() {
        Ok(p) => (p.x / s, p.y / s),
        Err(_) => (0.0, 0.0),
    }
}

pub fn primary_scale(app: &AppHandle) -> f64 {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0)
}

/// 找到 CSS 坐标所在的显示器
pub fn monitor_at_css(app: &AppHandle, x: f64, y: f64) -> Option<Monitor> {
    #[cfg(target_os = "macos")]
    {
        app.monitor_from_point(x, y).ok().flatten()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let s = primary_scale(app);
        app.monitor_from_point(x * s, y * s).ok().flatten()
    }
}

/// 包含某物理坐标点的显示器的 scale（用于物理↔CSS 换算）
pub fn scale_at_phys(app: &AppHandle, x: i32, y: i32) -> f64 {
    if let Ok(monitors) = app.available_monitors() {
        for m in monitors {
            let p = m.position();
            let sz = m.size();
            let (px, py) = (p.x as f64, p.y as f64);
            let (sw, sh) = (sz.width as f64, sz.height as f64);
            let (fx, fy) = (x as f64, y as f64);
            if fx >= px && fx < px + sw && fy >= py && fy < py + sh {
                return m.scale_factor();
            }
        }
    }
    primary_scale(app)
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct RectCss {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 显示器工作区（排除菜单栏/Dock），换算成 CSS px
pub fn work_area_css(m: &Monitor) -> RectCss {
    let wa = m.work_area();
    let s = m.scale_factor();
    RectCss {
        x: wa.position.x as f64 / s,
        y: wa.position.y as f64 / s,
        w: wa.size.width as f64 / s,
        h: wa.size.height as f64 / s,
    }
}

/// CSS 坐标 → Tauri 物理坐标
pub fn to_phys(x: f64, y: f64, s: f64) -> tauri::PhysicalPosition<i32> {
    tauri::PhysicalPosition::new((x * s).round() as i32, (y * s).round() as i32)
}
