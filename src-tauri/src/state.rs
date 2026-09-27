use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::geom::MonitorKey;
use crate::usage::{UsageConfig, UsageData};

/// 拖动状态：窗口和鼠标都使用 geom 定义的统一桌面坐标系。
pub struct DragState {
    /// 按下时全局原生光标位置（macOS 点坐标 / 其他平台桌面物理像素）
    pub sx: f64,
    pub sy: f64,
    /// 按下时窗口左上角，单位与 sx/sy 相同
    pub x: f64,
    pub y: f64,
    /// 最近一次拖动 IPC 时间（主循环泄漏兜底用）
    pub last_move: Instant,
    /// 上一次 setPosition 的期望位置（用于识别系统弹回）
    pub expect: Option<(f64, f64)>,
    /// 期望位置所属显示器
    pub expect_monitor: Option<MonitorKey>,
    /// 最后一次系统确认的稳定 x（弹回学习的边界依据）
    pub stable_x: f64,
    /// 稳定位置所属显示器；不跨屏拿旧落点学习边界
    pub stable_monitor: Option<MonitorKey>,
}

/// 拖动中动态学习的系统真实边界，按显示器隔离。
#[derive(Clone, Copy)]
pub struct Limit {
    pub monitor: MonitorKey,
    pub min_x: f64,
    pub max_x: f64,
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
    pub pet_limit: Mutex<Vec<Limit>>,
    pub move_confirm: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    /// 订阅用量共享状态（轮询器写，命令/事件读）
    pub usage: Mutex<UsageCore>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            drag: Mutex::new(None),
            pet_limit: Mutex::new(Vec::new()),
            move_confirm: Mutex::new(None),
            usage: Mutex::new(UsageCore::default()),
        }
    }
}

impl Limit {
    pub fn open(monitor: MonitorKey) -> Self {
        Self {
            monitor,
            min_x: f64::NEG_INFINITY,
            max_x: f64::INFINITY,
        }
    }
}
