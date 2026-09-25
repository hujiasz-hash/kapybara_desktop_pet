use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::usage::{UsageConfig, UsageData};

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

/// 订阅用量的内存态：当前配置 + 各账号最近一次查询结果
pub struct UsageCore {
    pub config: UsageConfig,
    /// "copilot" | "zhipu" | "custom" → 最近一次查询结果
    pub usages: BTreeMap<String, UsageData>,
    pub fetched_at: Option<i64>,
}

impl Default for UsageCore {
    fn default() -> Self {
        Self {
            config: UsageConfig::default(),
            usages: BTreeMap::new(),
            fetched_at: None,
        }
    }
}

pub struct AppState {
    pub drag: Mutex<Option<DragState>>,
    pub pet_limit: Mutex<Option<Limit>>,
    pub move_confirm: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// 订阅用量共享状态（轮询器写，命令/事件读）
    pub usage: Mutex<UsageCore>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            drag: Mutex::new(None),
            pet_limit: Mutex::new(None),
            move_confirm: Mutex::new(None),
            usage: Mutex::new(UsageCore::default()),
        }
    }
}

impl Limit {
    pub fn open() -> Self {
        Self { min_x: i32::MIN / 2, max_x: i32::MAX / 2 }
    }
}
