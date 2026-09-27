//! 桌面坐标统一层
//!
//! 拖动、全局光标、屏幕工作区与窗口位置必须使用同一坐标系：
//! - macOS：全局逻辑点（Cocoa/Quartz 的 desktop points），不把不同显示器的 Retina scale 混在一起。
//! - Windows：DPI-aware 虚拟桌面的物理像素，直接使用原生光标、窗口与工作区坐标。
//! - 其他桌面平台：沿用 Tauri 的 PhysicalPosition；混合 DPI 行为需按具体后端验证。
//! Tauri 的窗口位置在 macOS 上是按窗口当前屏幕 scale 缩放后的 PhysicalPosition，
//! 因此读回时按窗口自己的 scale 还原；写入时传 LogicalPosition，跨屏不依赖旧屏 scale。

use tauri::{AppHandle, Monitor, PhysicalPosition, WebviewWindow};

pub const PET_SIZE: f64 = 110.0;
/// 订阅配置面板（原问答窗尺寸，内容变高一点）
pub const CONFIG_W: f64 = 640.0;
pub const CONFIG_H: f64 = 470.0;
/// 悬浮用量面板
pub const USAGE_W: f64 = 300.0;
pub const USAGE_H: f64 = 420.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesktopRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 屏幕指纹：用原生屏幕几何和 scale 标识一块显示器，避免把一块屏学到的限制套给别的屏。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MonitorKey {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_milli: u32,
}

/// 窗口在统一桌面坐标系中的位置和尺寸；logical_scale 用于把 UI 逻辑尺寸换成桌面坐标单位。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowGeometry {
    pub rect: DesktopRect,
    pub logical_scale: f64,
}

/// 全局原生光标位置。macOS 为全局点坐标，Windows 为虚拟桌面物理像素。
pub fn cursor_desktop(_app: &AppHandle) -> Option<(f64, f64)> {
    #[cfg(target_os = "macos")]
    {
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
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return None;
            }
            let point = CGEventGetLocation(event);
            CFRelease(event);
            Some((point.x, point.y))
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        _app.cursor_position().ok().map(|p| (p.x, p.y))
    }
}

/// LogicalPosition 对应本平台桌面坐标系中的位移/尺寸。
/// macOS 的统一单位是逻辑点；Windows 的统一单位是虚拟桌面物理像素。
pub fn logical_to_desktop(value: f64, monitor_scale: f64) -> f64 {
    #[cfg(target_os = "macos")]
    {
        let _ = monitor_scale;
        value
    }
    #[cfg(not(target_os = "macos"))]
    {
        value * monitor_scale
    }
}

/// 屏幕工作区转换成统一桌面坐标。Tauri 在 macOS 上返回按该屏 scale 缩放的物理工作区，
/// Windows 的工作区则是虚拟桌面物理像素。
pub fn work_area_desktop(monitor: &Monitor) -> DesktopRect {
    let wa = monitor.work_area();
    let scale = monitor.scale_factor();
    DesktopRect {
        x: physical_to_desktop(wa.position.x as f64, scale),
        y: physical_to_desktop(wa.position.y as f64, scale),
        w: physical_to_desktop(wa.size.width as f64, scale),
        h: physical_to_desktop(wa.size.height as f64, scale),
    }
}

/// Physical 值还原到统一桌面坐标：macOS 需除以所属屏幕 scale，其余平台物理桌面坐标已统一。
pub fn physical_to_desktop(value: f64, monitor_scale: f64) -> f64 {
    #[cfg(target_os = "macos")]
    {
        value / monitor_scale.max(0.01)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = monitor_scale;
        value
    }
}

/// 窗口的 PhysicalPosition 还原到统一桌面坐标。
pub fn position_desktop(window: &WebviewWindow, position: &PhysicalPosition<i32>) -> (f64, f64) {
    let scale = window.scale_factor().unwrap_or(1.0);
    (
        physical_to_desktop(position.x as f64, scale),
        physical_to_desktop(position.y as f64, scale),
    )
}

/// 窗口当前位置与实际尺寸转换到统一桌面坐标。
pub fn window_geometry_desktop(window: &WebviewWindow) -> Option<WindowGeometry> {
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let (x, y) = position_desktop(window, &position);
    let scale = window.scale_factor().unwrap_or(1.0).max(0.01);
    #[cfg(target_os = "macos")]
    let logical_scale = 1.0;
    #[cfg(not(target_os = "macos"))]
    let logical_scale = scale;
    Some(WindowGeometry {
        rect: DesktopRect {
            x,
            y,
            w: physical_to_desktop(size.width as f64, scale),
            h: physical_to_desktop(size.height as f64, scale),
        },
        logical_scale,
    })
}

/// 根据全局桌面坐标找到屏幕；指针落在显示器布局的空隙时选最近屏幕，而不是误退回主屏。
pub fn monitor_at_desktop(app: &AppHandle, x: f64, y: f64) -> Option<Monitor> {
    if let Ok(Some(monitor)) = app.monitor_from_point(x, y) {
        return Some(monitor);
    }
    if let Ok(monitors) = app.available_monitors() {
        let nearest = monitors.into_iter().min_by(|a, b| {
            distance_to_rect(x, y, work_area_desktop(a)).total_cmp(&distance_to_rect(
                x,
                y,
                work_area_desktop(b),
            ))
        });
        if nearest.is_some() {
            return nearest;
        }
    }
    app.primary_monitor().ok().flatten()
}

fn distance_to_rect(x: f64, y: f64, rect: DesktopRect) -> f64 {
    let dx = if x < rect.x {
        rect.x - x
    } else if x > rect.x + rect.w {
        x - (rect.x + rect.w)
    } else {
        0.0
    };
    let dy = if y < rect.y {
        rect.y - y
    } else if y > rect.y + rect.h {
        y - (rect.y + rect.h)
    } else {
        0.0
    };
    dx * dx + dy * dy
}

pub fn monitor_key(monitor: &Monitor) -> MonitorKey {
    let position = monitor.position();
    let size = monitor.size();
    MonitorKey {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        scale_milli: (monitor.scale_factor() * 1000.0).round().max(0.0) as u32,
    }
}

/// 将桌宠窗口放到统一桌面坐标系指定的位置。
/// macOS 传全局逻辑点；Windows 传虚拟桌面的物理像素。
pub fn set_position_desktop(window: &WebviewWindow, x: f64, y: f64) -> tauri::Result<()> {
    #[cfg(target_os = "macos")]
    {
        window.set_position(tauri::LogicalPosition::new(x, y))
    }
    #[cfg(not(target_os = "macos"))]
    {
        window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_keys_distinguish_external_displays_and_scale_changes() {
        let primary = MonitorKey {
            x: 0,
            y: 0,
            width: 3024,
            height: 1964,
            scale_milli: 2000,
        };
        let external = MonitorKey {
            x: 3024,
            y: 0,
            width: 1920,
            height: 1080,
            scale_milli: 1000,
        };
        let scaled_external = MonitorKey {
            scale_milli: 1500,
            ..external
        };
        assert_ne!(primary, external);
        assert_ne!(external, scaled_external);
    }

    #[test]
    fn retina_and_standard_scale_edges_share_one_macos_coordinate() {
        let retina_right_edge = physical_to_desktop(3024.0, 2.0);
        let external_left_edge = physical_to_desktop(1512.0, 1.0);
        #[cfg(target_os = "macos")]
        assert_eq!(retina_right_edge, external_left_edge);
        #[cfg(not(target_os = "macos"))]
        assert_ne!(retina_right_edge, external_left_edge);
    }

    #[test]
    fn logical_size_maps_to_the_platform_desktop_unit() {
        let one_x = logical_to_desktop(110.0, 1.0);
        let two_x = logical_to_desktop(110.0, 2.0);
        #[cfg(target_os = "macos")]
        assert_eq!(one_x, two_x);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(two_x, one_x * 2.0);
    }

    #[test]
    fn distance_to_rect_uses_negative_and_offset_screen_origins() {
        let left_screen = DesktopRect {
            x: -1920.0,
            y: -200.0,
            w: 1920.0,
            h: 1080.0,
        };
        let right_screen = DesktopRect {
            x: 0.0,
            y: 0.0,
            w: 1512.0,
            h: 982.0,
        };
        assert_eq!(distance_to_rect(-100.0, 100.0, left_screen), 0.0);
        assert_eq!(distance_to_rect(1600.0, 100.0, right_screen), 88.0 * 88.0);
    }
}
