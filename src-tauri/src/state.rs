use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use tauri::Manager;

/// 当前进行中的回答，stop 时按类型中止
pub enum Active {
    /// pollinations 任务句柄（abort 即中止）
    Pollinations(tauri::async_runtime::JoinHandle<()>),
    /// agy 子进程（存于 child 字段，kill 即中止）
    Agy,
}

/// 拖动状态：绝对坐标法（按下时窗口位置 + 鼠标净位移），见 main.js v1.3 的设计
pub struct DragState {
    /// 按下时鼠标位置（CSS px）
    pub sx: f64,
    pub sy: f64,
    /// 按下时窗口位置（Tauri 物理坐标）
    pub x: i32,
    pub y: i32,
    /// 最近一次拖动 IPC 时间（主循环泄漏兜底用）
    pub last_move: Instant,
    /// 上一次 setPosition 的期望位置（用于识别系统弹回）
    pub expect: Option<(i32, i32)>,
    /// 最后一次系统确认的稳定 x（弹回学习的边界依据）
    pub stable_x: i32,
}

/// 拖动中动态学习的系统真实边界（台前调度条不反映在 workArea 里，见 README v2.2~v2.4）
#[derive(Clone, Copy)]
pub struct Limit {
    pub min_x: i32,
    pub max_x: i32,
}

pub struct AppState {
    pub busy: AtomicBool,
    pub active: Mutex<Option<Active>>,
    pub child: Mutex<Option<tokio::process::Child>>,
    pub drag: Mutex<Option<DragState>>,
    pub pet_limit: Mutex<Option<Limit>>,
    pub move_confirm: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// KB_TEST_ASK 自动化测试：收集回答文本 / 统计完成次数
    pub test_buf: Mutex<String>,
    pub test_done: AtomicU32,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            busy: AtomicBool::new(false),
            active: Mutex::new(None),
            child: Mutex::new(None),
            drag: Mutex::new(None),
            pet_limit: Mutex::new(None),
            move_confirm: Mutex::new(None),
            test_buf: Mutex::new(String::new()),
            test_done: AtomicU32::new(0),
        }
    }
}

impl Limit {
    pub fn open() -> Self {
        Self { min_x: i32::MIN / 2, max_x: i32::MAX / 2 }
    }
}

pub fn test_done_count(app: &tauri::AppHandle) -> u32 {
    app.state::<AppState>().test_done.load(Ordering::SeqCst)
}
