//! 订阅用量（v5.0）：配置持久化 + 三类账号查询 + 轮询 + 悬停面板窗口管理
//!
//! 供应商与 API（实现依据：ClaudeBar zai/claude provider 文档、自研 Obsidian usage-hud 插件）：
//!   1. GitHub Copilot（默认）
//!      GET https://api.github.com/users/{user}/settings/billing/premium_request/usage
//!      headers: Accept: application/vnd.github+json / Authorization: Bearer {PAT} / X-GitHub-Api-Version: 2022-11-28
//!      解析 usageItems[]：sku 含 "Premium" 的 grossQuantity 求和 = 已用；首个 limit>0 的条目为额度；
//!      无 limit 时按套餐兜底（free 50 / pro 300 / pro+ 1500 / business 300 / enterprise 1000）。
//!   2. 智谱中国（GLM Coding Plan，极简：只填 Key）
//!      GET {platform}/api/monitor/usage/quota/limit
//!      headers: Authorization: {Key}（raw，无 Bearer）/ Content-Type: application/json
//!      解析 data.limits[]：type+unit → 5h(unit3) / 周(unit6) / 月(unit7) / MCP(TIME_LIMIT)，
//!      percentage 为已用百分比，nextResetTime 兼容 epoch-ms / epoch-s / ISO 字符串。
//!   3. 自定义（博世 MFC 等，URL 适配）
//!      host 含 aigc.bosch.com.cn → 博世 MFC：
//!        GET {origin}/llmservice/api/v1/client/usage?dateType={month|day}&startDate=..&endDate=..
//!        headers: Authorization: Bearer {Key} / Accept: application/json
//!        解析 code==200 的 data：cost/totalCost、totalTokens、details[](model/cost/totalTokens)
//!      host 含 bigmodel.cn / z.ai → 复用智谱查询（用该 host 作平台）
//!      其他 → 仅保存配置，标注"暂不支持用量查询"

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::geom;
use crate::state::AppState;

// ============================ 配置 ============================

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CopilotCfg {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub username: String,
    /// free | pro | pro+ | business | enterprise（仅当接口没回 limit 时兜底额度）
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub pat: String,
    /// 凭据来源：auto（自动发现 pi / copilot-cli，默认）| token（用下面的 token）| pat（旧版账单接口）
    #[serde(default)]
    pub source: String,
    /// 手填的 OAuth token（gho_ / ghu_）：配额接口只认它，细粒度 PAT 会 401
    #[serde(default)]
    pub token: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct ZhipuCfg {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub key: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct CustomCfg {
    #[serde(default)]
    pub enabled: bool,
    /// 任意形式的地址：裸域名 / 带 scheme / 带路径（见 normalize + detect）
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct UsageConfig {
    #[serde(default)]
    pub copilot: CopilotCfg,
    #[serde(default)]
    pub zhipu: ZhipuCfg,
    #[serde(default)]
    pub custom: CustomCfg,
}

impl Default for UsageConfig {
    fn default() -> Self {
        Self {
            // Copilot 是默认供应商：首次启动即启用（填好用户名+PAT 即出数）
            copilot: CopilotCfg { enabled: true, ..Default::default() },
            zhipu: ZhipuCfg::default(),
            custom: CustomCfg::default(),
        }
    }
}

fn poll_secs() -> Duration {
    let s = std::env::var("KB_USAGE_POLL_SEC")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300)
        .clamp(60, 86400);
    Duration::from_secs(s)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ============================ 用量快照 ============================

#[derive(Serialize, Clone)]
pub struct ModelRow {
    pub model: String,
    pub value: f64,
}

#[derive(Serialize, Clone)]
pub struct QuotaWindow {
    /// "5h" | "week" | "month" | "mcp"
    pub kind: String,
    /// 已用百分比 0~100
    pub pct: f64,
    pub reset_at_ms: Option<i64>,
}

#[derive(Serialize, Clone)]
pub struct MfcPeriod {
    pub cost: f64,
    pub total_tokens: f64,
    pub models: Vec<ModelRow>,
}

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UsageData {
    /// 刚启用还没刷过
    Pending,
    /// 自定义 URL 无法识别用量接口（配置已保存）
    Unsupported { message: String },
    Error { message: String },
    Copilot {
        used: f64,
        limit: f64,
        tier: String,
        /// 账期 YYYY-MM
        period: String,
        models: Vec<ModelRow>,
        reset_at_ms: Option<i64>,
        /// true = 新计量单位 AI Credits（2026-06 起），false = 旧版 premium request
        #[serde(default)]
        credits: bool,
        /// 剩余额度（仅 credits 模式有意义）
        #[serde(default)]
        remaining: f64,
        /// 组织席位归属，如 bosch-copilot（个人订阅为空）
        #[serde(default)]
        org: String,
        /// 凭据来源说明，如 "pi OAuth" / "copilot-cli 钥匙串" / "手填 Token"
        #[serde(default)]
        source: String,
    },
    Zhipu {
        level: String,
        windows: Vec<QuotaWindow>,
    },
    Mfc {
        month: MfcPeriod,
        today: Option<MfcPeriod>,
    },
}

/// Copilot 客户端约定的接口版本（与 GitHubCopilotChat 0.35.0 一致）
const COPILOT_API_VERSION: &str = "2026-06-01";

impl UsageData {
    pub fn error(msg: impl Into<String>) -> Self {
        UsageData::Error { message: msg.into() }
    }
    pub fn unsupported(msg: impl Into<String>) -> Self {
        UsageData::Unsupported { message: msg.into() }
    }
}

// ============================ URL 适配 ============================

#[derive(Debug, PartialEq)]
pub enum CustomKind {
    Mfc,
    Zhipu,
    Unknown,
}

/// 识别自定义地址 → (种类, 平台 origin, 展示名)
/// 兼容：裸域名 / 缺 scheme / 带任意路径（含完整接口地址）/ 带尾斜杠
pub fn detect_custom(raw: &str) -> (CustomKind, String, String) {
    let mut s = raw.trim().to_string();
    if s.is_empty() {
        return (CustomKind::Unknown, String::new(), String::new());
    }
    if !s.to_ascii_lowercase().starts_with("http://") && !s.to_ascii_lowercase().starts_with("https://") {
        s = format!("https://{}", s);
    }
    let origin = match reqwest::Url::parse(&s) {
        Ok(u) => match (u.scheme(), u.port()) {
            (sch, Some(port)) => format!("{}://{}:{}", sch, u.host_str().unwrap_or(""), port),
            (sch, None) => format!("{}://{}", sch, u.host_str().unwrap_or("")),
        },
        Err(_) => return (CustomKind::Unknown, String::new(), String::new()),
    };
    let host = reqwest::Url::parse(&s)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default();

    if host.contains("aigc.bosch.com.cn") || host.contains("bosch.com.cn") {
        (CustomKind::Mfc, origin, "博世 MFC".into())
    } else if host.contains("bigmodel.cn") {
        (CustomKind::Zhipu, origin, "智谱 GLM".into())
    } else if host.ends_with("z.ai") || host.contains("api.z.ai") {
        (CustomKind::Zhipu, origin, "智谱国际 GLM".into())
    } else {
        (CustomKind::Unknown, origin, "未识别".into())
    }
}

// ============================ 持久化 ============================

fn store_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("usage.json"))
}

pub fn load_config(app: &AppHandle) -> UsageConfig {
    let Some(path) = store_path(app) else { return UsageConfig::default() };
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            println!("[usage] 配置解析失败({})，用默认配置: {}", e, path.display());
            UsageConfig::default()
        }),
        Err(_) => UsageConfig::default(),
    }
}

pub fn save_config(app: &AppHandle, cfg: &UsageConfig) {
    let Some(path) = store_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(cfg) {
        Ok(text) => {
            if let Err(e) = std::fs::write(&path, text) {
                println!("[usage] 配置写入失败: {}", e);
            }
        }
        Err(e) => println!("[usage] 配置序列化失败: {}", e),
    }
}

/// 全量快照（配置 + 各账号用量 + 更新时间），供配置面板 / 悬浮面板初始化与事件更新
pub fn state_json(app: &AppHandle) -> Value {
    let core = &app.state::<AppState>().usage;
    let st = core.lock().unwrap();
    let mut usages = st.usages.clone();
    // 已启用但尚无查询结果的账号 → Pending（面板显示"查询中…"）
    if st.config.copilot.enabled && !usages.contains_key("copilot") {
        usages.insert("copilot".into(), UsageData::Pending);
    }
    if st.config.zhipu.enabled && !usages.contains_key("zhipu") {
        usages.insert("zhipu".into(), UsageData::Pending);
    }
    if st.config.custom.enabled && !usages.contains_key("custom") {
        usages.insert("custom".into(), UsageData::Pending);
    }
    json!({
        "config": st.config,
        "usages": usages,
        "fetchedAt": st.fetched_at,
    })
}

// ============================ 查询实现 ============================

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .user_agent("kapybara-buddy/5.0")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// 数值容错：数字或数字字符串 → f64（照抄 usage-hud 的 w()）
fn f64_of(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(Value::String(s)) => s.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn opt_f64(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn str_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// nextResetTime 兼容：epoch-ms 数字 / epoch-s 数字 / ISO8601 字符串
fn parse_reset_ms(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(_)) => {
            let f = f64_of(v);
            if f <= 0.0 {
                None
            } else if f > 1e12 {
                Some(f as i64) // 已是毫秒
            } else if f > 1e9 {
                Some((f * 1000.0) as i64) // 秒
            } else {
                None
            }
        }
        Some(Value::String(s)) => {
            let t = s.trim();
            if let Ok(n) = t.parse::<f64>() {
                return parse_reset_ms(Some(&json!(n)));
            }
            if let Ok(d) = chrono::DateTime::parse_from_rfc3339(t) {
                return Some(d.timestamp_millis());
            }
            // 裸日期 "2026-10-01"（Copilot 的 quota_reset_date）→ 当天 0 点 UTC
            NaiveDate::parse_from_str(t, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|dt| dt.and_utc().timestamp_millis())
        }
        _ => None,
    }
}

/// Copilot 额度按套餐兜底
fn tier_limit(tier: &str) -> f64 {
    match tier.trim().to_ascii_lowercase().as_str() {
        "free" => 50.0,
        "pro+" | "pro_plus" => 1500.0,
        "business" => 300.0,
        "enterprise" => 1000.0,
        _ => 300.0, // pro
    }
}

/// 下月 1 日 0 点 UTC（Copilot 月度重置点，照抄 usage-hud 的 Date.UTC 算法）
fn next_month_first_utc_ms() -> Option<i64> {
    let now = Utc::now();
    let (y, m) = if now.month() == 12 { (now.year() + 1, 1) } else { (now.year(), now.month() + 1) };
    Utc.with_ymd_and_hms(y, m, 1, 0, 0, 0).single().map(|t| t.timestamp_millis())
}

/// 旧版账单接口（premium request，2026-06 前的计量单位）：只对个人订阅有效，
/// 组织分配席位的账号这里永远返回空 usageItems，因此仅为兜底保留
async fn fetch_copilot_billing(username: &str, pat: &str, tier: &str) -> UsageData {
    let url = format!(
        "https://api.github.com/users/{}/settings/billing/premium_request/usage",
        username.trim()
    );
    let resp = client()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("Authorization", format!("Bearer {}", pat.trim()))
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageData::error(format!("网络错误: {}", e)),
    };
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return UsageData::error(format!("Copilot 认证失败 (HTTP {})，检查用户名与 PAT", status));
    }
    if status != 200 {
        return UsageData::error(format!("Copilot 接口返回 HTTP {}", status));
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return UsageData::error(format!("Copilot 返回解析失败: {}", e)),
    };

    let empty = Vec::new();
    let items = body.get("usageItems").and_then(|v| v.as_array()).unwrap_or(&empty);
    let mut used = 0.0f64;
    let mut limit = 0.0f64;
    let mut saw_premium = false;
    let mut models: Vec<ModelRow> = Vec::new();
    for it in items {
        let sku = str_of(it.get("sku")).to_ascii_lowercase();
        let qty = f64_of(it.get("grossQuantity"));
        if sku.contains("premium") {
            saw_premium = true;
            used += qty;
            if limit <= 0.0 {
                limit = opt_f64(it.get("limit")).unwrap_or(0.0);
            }
        }
        let model = str_of(it.get("model"));
        if !model.is_empty() && qty > 0.0 {
            models.push(ModelRow { model: model.chars().take(28).collect(), value: qty });
        }
    }
    if !saw_premium && !items.is_empty() {
        used = models.iter().map(|m| m.value).sum();
    }
    if limit <= 0.0 {
        limit = tier_limit(tier);
    }
    // 模型聚合（同 model 求和）+ 降序取前 5
    let mut agg: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for m in models {
        *agg.entry(m.model).or_insert(0.0) += m.value;
    }
    let mut models: Vec<ModelRow> = agg.into_iter().map(|(model, value)| ModelRow { model, value }).collect();
    models.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap_or(std::cmp::Ordering::Equal));
    models.truncate(5);

    let tp = body.get("timePeriod");
    let period = match tp {
        Some(tp) => {
            let y = str_of(tp.get("year"));
            let m = str_of(tp.get("month"));
            if m.is_empty() {
                y
            } else {
                format!("{}-{}", y, m)
            }
        }
        None => String::new(),
    };

    UsageData::Copilot {
        used,
        limit,
        tier: tier.to_string(),
        period,
        models,
        reset_at_ms: next_month_first_utc_ms(),
        credits: false,
        remaining: (limit - used).max(0.0),
        org: String::new(),
        source: "账单接口 (PAT)".into(),
    }
}

// ==================== Copilot 配额（AI Credits） ====================
//
// 2026-06-01 起 Copilot 改为按量计费：旧的 premium request 账单接口
// （/users/{u}/settings/billing/premium_request/usage）对「组织分配的席位」返回 200 但
// usageItems 为空，永远显示 0/300。真正的额度只能从 Copilot 客户端同款内部接口读：
//
//   GET https://api.github.com/copilot_internal/user
//   Authorization: token <GitHub OAuth token>
//   → quota_snapshots.premium_interactions { entitlement, credits_used, remaining, ... }
//
// 注意：这个接口只认 OAuth token（gho_/ghu_），不认细粒度 PAT，也不认 Copilot 会话
// token（tid=…，会 401 Bad credentials）。

/// 读 pi 的登录态（~/.pi/agent/auth.json → github-copilot.refresh 才是 OAuth token，
/// access 字段是 tid=… 的会话 token，不能用）
fn token_from_pi() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let raw = std::fs::read_to_string(format!("{}/.pi/agent/auth.json", home)).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    let node = v.get("github-copilot")?;
    for k in ["refresh", "access"] {
        let t = str_of(node.get(k));
        if usable_token(&t) {
            return Some(t);
        }
    }
    // 兜底：任何字段里像 OAuth token 的值
    node.as_object()?.values().map(|x| str_of(Some(x))).find(|t| usable_token(t))
}

/// 读 macOS 钥匙串里的 copilot-cli 登录（Copilot CLI 用 gho_ token）
fn token_from_keychain() -> Option<String> {
    let out = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "copilot-cli", "-w"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if usable_token(&t) { Some(t) } else { None }
}

/// 只用 OAuth token：空的 / tid=… 的会话 token 一律不要（那个必然 401）
fn usable_token(t: &str) -> bool {
    let t = t.trim();
    !t.is_empty() && !t.starts_with("tid=") && t.len() > 20
}

/// 凭据发现顺序：手填 token → pi 登录态 → copilot-cli 钥匙串
/// 返回 (token, 展示用来源名)
fn resolve_copilot_token(cfg: &CopilotCfg) -> Option<(String, String)> {
    let manual = cfg.token.trim();
    if usable_token(manual) {
        return Some((manual.to_string(), "手填 Token".into()));
    }
    if cfg.source.trim() == "token" {
        return None; // 明确指定手填，但没填/不合法
    }
    if let Some(t) = token_from_pi() {
        return Some((t, "pi 登录态".into()));
    }
    token_from_keychain().map(|t| (t, "copilot-cli 钥匙串".into()))
}

/// 配额快照 → UsageData（纯函数，便于单测）
fn copilot_quota_to_usage(body: &Value, source: &str) -> UsageData {
    if let Some(msg) = body.get("message").and_then(|m| m.as_str()) {
        // GitHub 的 404/403 都长这样，权限不足一律伪装成 404
        return UsageData::error(format!("Copilot 配额接口: {}", msg));
    }
    let snaps = body.get("quota_snapshots");
    // 新版是 premium_interactions（AI Credits），老账号可能还是 premium_requests
    let snap = snaps
        .and_then(|s| s.get("premium_interactions").or_else(|| s.get("premium_requests")));
    let Some(snap) = snap else {
        return UsageData::unsupported("Copilot 返回里没有额度快照（账号可能没有 Copilot 席位）");
    };

    let entitlement = opt_f64(snap.get("entitlement")).unwrap_or(0.0);
    let remaining_raw = opt_f64(snap.get("remaining"));
    let unlimited = snap.get("unlimited").and_then(|v| v.as_bool()).unwrap_or(false);
    let used = match opt_f64(snap.get("credits_used")).or_else(|| opt_f64(snap.get("used"))) {
        Some(u) => u,
        None => remaining_raw.map(|r| (entitlement - r).max(0.0)).unwrap_or(0.0),
    };
    let remaining = remaining_raw.unwrap_or_else(|| (entitlement - used).max(0.0));
    // 不限量 → limit=0：前端按「不限额」渲染，不算百分比
    let limit = if unlimited { 0.0 } else { entitlement };

    let tier = {
        let p = str_of(body.get("copilot_plan"));
        if p.is_empty() { "copilot".to_string() } else { p }
    };
    // 组织席位（bosch-copilot）；个人订阅为空
    let org = body
        .get("organization_login_list")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .map(|v| str_of(Some(v)))
        .unwrap_or_default();

    let reset_at_ms = parse_reset_ms(body.get("quota_reset_date_utc"))
        .or_else(|| parse_reset_ms(body.get("quota_reset_date")))
        .or_else(next_month_first_utc_ms);

    UsageData::Copilot {
        used,
        limit,
        tier: tier.to_ascii_uppercase(),
        period: String::new(),
        models: Vec::new(),
        reset_at_ms,
        credits: true,
        remaining,
        org,
        source: source.to_string(),
    }
}

/// 走内部配额接口（Copilot 客户端同款），只要拿得到 OAuth token 就能读到真实 AI Credits
async fn fetch_copilot_quota(token: &str, source: &str) -> UsageData {
    let resp = client()
        .get("https://api.github.com/copilot_internal/user")
        .header("Accept", "application/json")
        .header("Authorization", format!("token {}", token.trim()))
        .header("User-Agent", "GitHubCopilotChat/0.35.0")
        .header("Editor-Version", "vscode/1.107.0")
        .header("Editor-Plugin-Version", "copilot-chat/0.35.0")
        .header("Copilot-Integration-Id", "vscode-chat")
        .header("X-GitHub-Api-Version", COPILOT_API_VERSION)
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageData::error(format!("网络错误: {}", e)),
    };
    let status = resp.status().as_u16();
    if status == 401 {
        return UsageData::error(format!("Copilot 配额认证失败 (HTTP 401)，{}已失效", source));
    }
    if status != 200 {
        return UsageData::error(format!("Copilot 配额接口返回 HTTP {}", status));
    }
    match resp.json::<Value>().await {
        Ok(v) => copilot_quota_to_usage(&v, source),
        Err(e) => UsageData::error(format!("Copilot 配额返回解析失败: {}", e)),
    }
}

/// Copilot 取数总入口：优先配额接口（真实 AI Credits），失败再退旧的账单接口（需要 PAT）
async fn fetch_copilot(cfg: &CopilotCfg) -> UsageData {
    let legacy = cfg.source.trim() == "pat";
    let mut quota_err: Option<UsageData> = None;

    if !legacy {
        match resolve_copilot_token(cfg) {
            Some((tok, src)) => {
                let r = fetch_copilot_quota(&tok, &src).await;
                if !matches!(r, UsageData::Error { .. } | UsageData::Unsupported { .. }) {
                    return r;
                }
                quota_err = Some(r);
            }
            None => {
                quota_err = Some(UsageData::error(
                    "没找到 Copilot 凭据：请在配置里粘一个 OAuth token（gho_/ghu_），\
                     或先用 pi / Copilot CLI 登录一次",
                ));
            }
        }
    }

    // 兜底：旧的 premium request 账单接口（个人订阅或显式 source=pat）
    if !cfg.username.trim().is_empty() && !cfg.pat.trim().is_empty() {
        let pat = cfg.pat.clone();
        let r = fetch_copilot_billing(&cfg.username, &pat, &cfg.tier).await;
        if !matches!(r, UsageData::Error { .. }) {
            return r;
        }
        if quota_err.is_none() {
            return r;
        }
    }
    quota_err.unwrap_or_else(|| UsageData::error("Copilot 未配置凭据"))
}

async fn fetch_zhipu(platform: &str, key: &str) -> UsageData {
    let url = format!("{}/api/monitor/usage/quota/limit", platform.trim_end_matches('/'));
    let resp = client()
        .get(&url)
        .header("Authorization", key.trim()) // 智谱约定：raw key，无 Bearer
        .header("Content-Type", "application/json")
        .header("Accept-Language", "en-US,en")
        .send()
        .await;
    let resp = match resp {
        Ok(r) => r,
        Err(e) => return UsageData::error(format!("网络错误: {}", e)),
    };
    let status = resp.status().as_u16();
    if status == 401 || status == 403 {
        return UsageData::error(format!("智谱认证失败 (HTTP {})，检查 API Key", status));
    }
    if status != 200 {
        return UsageData::error(format!("智谱接口返回 HTTP {}", status));
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return UsageData::error(format!("智谱返回解析失败: {}", e)),
    };
    let ok = body.get("success").and_then(|v| v.as_bool()).unwrap_or(false)
        || body.get("code").and_then(|v| v.as_i64()) == Some(200);
    if !ok {
        let msg = str_of(body.get("msg"));
        return UsageData::error(if msg.is_empty() { "智谱接口返回失败".into() } else { msg });
    }
    let data = body.get("data");
    let limits = data.and_then(|d| d.get("limits")).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let mut windows: Vec<QuotaWindow> = Vec::new();
    for l in limits {
        let ltype = str_of(l.get("type")).to_ascii_uppercase();
        let unit = str_of(l.get("unit"));
        let kind = if ltype == "TIME_LIMIT" {
            "mcp"
        } else {
            match unit.as_str() {
                "6" => "week",
                "7" => "month",
                _ => "5h", // unit 3 / 旧响应缺 unit
            }
        };
        let pct = f64_of(l.get("percentage"));
        if ltype != "TIME_LIMIT" && pct <= 0.0 && f64_of(l.get("remaining")) <= 0.0 {
            continue; // 全 0 的空窗口不展示
        }
        windows.push(QuotaWindow {
            kind: kind.to_string(),
            pct: pct.clamp(0.0, 100.0),
            reset_at_ms: parse_reset_ms(l.get("nextResetTime")),
        });
    }
    UsageData::Zhipu {
        level: str_of(data.and_then(|d| d.get("level"))),
        windows,
    }
}

fn parse_mfc_data(data: Option<&Value>) -> MfcPeriod {
    let empty = Vec::new();
    let Some(d) = data else { return MfcPeriod { cost: 0.0, total_tokens: 0.0, models: vec![] } };
    // details 可能在 data.details 或 data.periods[0].details
    let details = d
        .get("details")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| {
            data.and_then(|d| d.get("periods"))
                .and_then(|p| p.as_array())
                .and_then(|a| a.first())
                .and_then(|p0| p0.get("details").and_then(|v| v.as_array()).cloned())
        })
        .unwrap_or(empty);
    let mut models: Vec<ModelRow> = details
        .iter()
        .filter_map(|d| {
            let model = str_of(d.get("model"));
            if model.is_empty() {
                return None;
            }
            Some(ModelRow { model: model.chars().take(28).collect(), value: f64_of(d.get("cost")) })
        })
        .collect();
    models.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap_or(std::cmp::Ordering::Equal));
    models.truncate(5);
    MfcPeriod {
        cost: f64_of(d.get("cost")),
        total_tokens: f64_of(d.get("totalTokens")),
        models,
    }
}

async fn fetch_mfc(origin: &str, key: &str) -> UsageData {
    // 本地日期：月账单 YYYY-MM，日账单 YYYY-MM-DD
    let now = Local::now();
    let month = now.format("%Y-%m").to_string();
    let day = now.format("%Y-%m-%d").to_string();
    let base = origin.trim_end_matches('/');
    let url = |date_type: &str, date: &str| {
        format!(
            "{}/llmservice/api/v1/client/usage?dateType={}&startDate={}&endDate={}",
            base, date_type, date, date
        )
    };

    let get = |u: String| {
        let u = u.clone();
        async move {
            client()
                .get(&u)
                .header("Authorization", format!("Bearer {}", key.trim()))
                .header("Accept", "application/json")
                .send()
                .await
        }
    };

    // 月账单（主请求，失败即报错）
    let resp = match get(url("month", &month)).await {
        Ok(r) => r,
        Err(e) => return UsageData::error(format!("网络错误: {}", e)),
    };
    let status = resp.status().as_u16();
    if status != 200 {
        return UsageData::error(format!("MFC 接口返回 HTTP {}", status));
    }
    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return UsageData::error("MFC 返回解析失败".to_string()),
    };
    let code = f64_of(body.get("code")) as u16;
    if code != 200 {
        let msg = str_of(body.get("msg"));
        return UsageData::error(if msg.is_empty() { "MFC 用量请求失败".to_string() } else { msg });
    }
    let month_usage = parse_mfc_data(body.get("data"));

    // 日账单（失败容忍，照抄 usage-hud）
    let today = match get(url("day", &day)).await {
        Ok(r) if r.status().as_u16() == 200 => match r.json::<Value>().await {
            Ok(v) if f64_of(v.get("code")) as u16 == 200 => {
                Some(parse_mfc_data(v.get("data")))
            }
            _ => None,
        },
        _ => None,
    };

    UsageData::Mfc { month: month_usage, today }
}

async fn fetch_custom(cfg: &CustomCfg) -> UsageData {
    let (kind, origin, label) = detect_custom(&cfg.url);
    match kind {
        CustomKind::Mfc => fetch_mfc(&origin, &cfg.key).await,
        CustomKind::Zhipu => fetch_zhipu(&origin, &cfg.key).await,
        CustomKind::Unknown => UsageData::Unsupported {
            message: format!(
                "该地址暂不支持用量查询（{}）——已保存配置",
                if label.is_empty() { cfg.url.trim() } else { &label }
            ),
        },
    }
}

// ============================ 刷新与轮询 ============================

/// 立即刷新所有启用账号（轮询器与手动刷新共用）
pub async fn refresh_all(app: &AppHandle) {
    let cfg = {
        let core = app.state::<AppState>();
        let st = core.usage.lock().unwrap();
        st.config.clone()
    };

    let mut usages: BTreeMap<String, UsageData> = BTreeMap::new();

    // Copilot：只要有凭据（手填 token / pi 登录态 / 钥匙串 / PAT 兜底）就刷
    let cp = &cfg.copilot;
    let cp_ready = !cp.token.trim().is_empty()
        || (!cp.username.trim().is_empty() && !cp.pat.trim().is_empty())
        || cp.source.trim() != "token";
    if cp.enabled && cp_ready {
        usages.insert("copilot".into(), fetch_copilot(cp).await);
    }
    if cfg.zhipu.enabled && !cfg.zhipu.key.trim().is_empty() {
        usages.insert("zhipu".into(), fetch_zhipu("https://open.bigmodel.cn", &cfg.zhipu.key).await);
    }
    if cfg.custom.enabled && !cfg.custom.url.trim().is_empty() && !cfg.custom.key.trim().is_empty() {
        usages.insert("custom".into(), fetch_custom(&cfg.custom).await);
    }

    {
        let core = &app.state::<AppState>().usage;
        let mut st = core.lock().unwrap();
        st.usages = usages;
        st.fetched_at = Some(now_ms());
    }
    let _ = app.emit("usage-update", state_json(app));
}

/// 启动轮询器：启动即刷一次，之后每 KB_USAGE_POLL_SEC（默认 300s）
pub fn start(app: AppHandle) {
    // 初始：磁盘配置 → 内存
    let cfg = load_config(&app);
    {
        let core = &app.state::<AppState>().usage;
        core.lock().unwrap().config = cfg;
    }
    println!("[kapybara-buddy] usage 轮询启动 (间隔 {:?})", poll_secs());
    tauri::async_runtime::spawn(async move {
        loop {
            refresh_all(&app).await;
            tokio::time::sleep(poll_secs()).await;
        }
    });
}

// ============================ 悬停面板窗口管理 ============================

static PANEL_KEEP: AtomicBool = AtomicBool::new(false);

/// 用量面板内容高度（渲染层上报的逻辑 px）：面板只显示已配置账号，
/// 卡片数不同高度就不同，窗口跟着收拢，避免下半截空着
static PANEL_H: AtomicU32 = AtomicU32::new(0);

/// 面板高度上下限：不低于一行提示，不高于屏幕常规可视高度
const PANEL_H_MIN: f64 = 120.0;
const PANEL_H_MAX: f64 = 900.0;

/// 把窗口高度设成 h（逻辑 px）。返回是否真的改了
fn apply_panel_height(panel: &tauri::WebviewWindow, h: f64) -> bool {
    let h = h.clamp(PANEL_H_MIN, PANEL_H_MAX);
    let cur = panel
        .outer_size()
        .ok()
        .map(|s| {
            let scale = panel.scale_factor().unwrap_or(1.0);
            s.height as f64 / scale
        })
        .unwrap_or(0.0);
    if (cur - h).abs() < 1.0 {
        return false;
    }
    let _ = panel.set_size(tauri::LogicalSize::new(geom::USAGE_W, h));
    true
}

/// 渲染层上报内容高度：面板没显示时立刻改（下次弹出就是对的），
/// 显示中也只允许变高——变矮可能把光标甩出悬停保持区，触发面板自己关闭
pub fn set_panel_height(app: &AppHandle, h: f64) {
    let h = h.clamp(PANEL_H_MIN, PANEL_H_MAX);
    if h < 1.0 {
        return;
    }
    PANEL_H.store(h as u32, Ordering::Relaxed);
    let Some(panel) = app.get_webview_window("usage") else { return };
    let visible = panel.is_visible().unwrap_or(false);
    if !visible {
        apply_panel_height(&panel, h);
        return;
    }
    let cur = panel
        .outer_size()
        .ok()
        .map(|s| s.height as f64 / panel.scale_factor().unwrap_or(1.0))
        .unwrap_or(0.0);
    if h > cur {
        apply_panel_height(&panel, h);
    }
}

fn position_and_show(app: &AppHandle) {
    let Some(panel) = app.get_webview_window("usage") else { return };
    let Some(pet) = app.get_webview_window("pet") else { return };
    let Ok(pos) = pet.outer_position() else { return };
    let ps = geom::scale_at_phys(app, pos.x, pos.y);
    let px = pos.x as f64 / ps;
    let py = pos.y as f64 / ps;

    // 先按上报的内容高度收拢窗口，再定位/显示（显示后再改会有跳变）
    let want_h = {
        let h = PANEL_H.load(Ordering::Relaxed) as f64;
        if h >= PANEL_H_MIN { h } else { geom::USAGE_H }
    };
    apply_panel_height(&panel, want_h);
    let h = panel
        .outer_size()
        .ok()
        .map(|s| s.height as f64 / panel.scale_factor().unwrap_or(1.0))
        .unwrap_or(want_h);

    let Some(mon) = geom::monitor_at_css(app, px + 55.0, py + 55.0)
        .or_else(|| app.primary_monitor().ok().flatten())
    else {
        return;
    };
    let wa = geom::work_area_css(&mon);
    let ms = mon.scale_factor();

    let mut x = px - geom::USAGE_W - 6.0; // 默认宠物左侧
    if x < wa.x + 4.0 {
        x = px + geom::PET_SIZE + 6.0; // 左边放不下放右侧
    }
    let y = (py - 40.0).max(wa.y + 4.0).min(wa.y + wa.h - h - 4.0);
    let _ = panel.set_position(geom::to_phys(x, y, ms));
    let _ = panel.show();
    println!("[usage] panel shown at ({:.0}, {:.0}) {:.0}x{:.0} mon_scale={}", x, y, geom::USAGE_W, h, ms);
    // 显示瞬间推一次最新快照
    let _ = app.emit("usage-update", state_json(app));
}

/// pet.html 悬停进出 / 面板自身悬停：true=保持显示，false=延迟 600ms 隐藏（面板 hover 可取消）
pub fn hover(app: &AppHandle, show: bool) {
    println!("[usage] hover show={}", show);
    if show {
        PANEL_KEEP.store(true, Ordering::SeqCst);
        position_and_show(app);
        return;
    }
    PANEL_KEEP.store(false, Ordering::SeqCst);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        if !PANEL_KEEP.load(Ordering::SeqCst) {
            if let Some(panel) = app2.get_webview_window("usage") {
                let _ = panel.hide();
            }
        }
    });
}

/// 悬停保持区外扩量（px）：贴到桌宠跟前就算命中，不必严格压在窗口矩形内
const HOVER_PAD: f64 = 8.0;

fn in_rect(x: f64, y: f64, rx: f64, ry: f64, rw: f64, rh: f64) -> bool {
    x >= rx - HOVER_PAD && x <= rx + rw + HOVER_PAD && y >= ry - HOVER_PAD && y <= ry + rh + HOVER_PAD
}

/// 光标（全局 CSS px）是否落在「悬停保持区」内：桌宠矩形 ∪ 用量面板矩形（仅面板可见时）
///
/// 悬停判定放在主进程，不用窗口的 mouseenter/mouseleave：桌宠窗是 focusable(false)
/// （设计上不许抢焦点），非 key 窗口的 WebView 命中测试不可靠——光标已经在宠物身上时
/// 窗口才出现、或拿不到 key，就收不到 mouseenter；而 mouseleave 同样不来，enter/leave
/// 配不上对就永久哑火（README v5.1）。全局光标本来每 120ms 轮询一次（转头就是靠它），
/// 直接拿它做命中测试，顺带把「光标从桌宠挪到面板上」这段也一起覆盖了。
pub fn cursor_in_hover_zone(app: &AppHandle, mx: f64, my: f64) -> bool {
    if let Some(pet) = app.get_webview_window("pet") {
        if let Ok(p) = pet.outer_position() {
            let s = geom::scale_at_phys(app, p.x, p.y);
            if in_rect(mx, my, p.x as f64 / s, p.y as f64 / s, geom::PET_SIZE, geom::PET_SIZE) {
                return true;
            }
        }
    }
    if let Some(panel) = app.get_webview_window("usage") {
        if panel.is_visible().unwrap_or(false) {
            // 用窗口实际尺寸：面板高度随账号数收拢，写死 USAGE_H 会多出一段"空气保持区"
            if let (Ok(p), Ok(sz)) = (panel.outer_position(), panel.outer_size()) {
                let s = geom::scale_at_phys(app, p.x, p.y);
                if in_rect(
                    mx,
                    my,
                    p.x as f64 / s,
                    p.y as f64 / s,
                    sz.width as f64 / s,
                    sz.height as f64 / s,
                ) {
                    return true;
                }
            }
        }
    }
    false
}

/// 拖动 / 打开配置面板时立即隐藏
pub fn hide_now(app: &AppHandle) {
    PANEL_KEEP.store(false, Ordering::SeqCst);
    if let Some(panel) = app.get_webview_window("usage") {
        let _ = panel.hide();
    }
}

// ============================ 单元测试 ============================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_detect_custom() {
        // 博世 MFC：各种写法都能识别
        for u in [
            "aigc.bosch.com.cn",
            "https://aigc.bosch.com.cn",
            "https://aigc.bosch.com.cn/llmservice/api/v1/client/usage",
            "http://aigc.bosch.com.cn/llmservice/api/v1/client/usage?dateType=month",
            "aigc.bosch.com.cn/",
        ] {
            let (k, origin, label) = detect_custom(u);
            assert_eq!(k, CustomKind::Mfc, "url={}", u);
            assert!(origin.starts_with("http"), "origin={}", origin);
            assert!(!origin.contains("/llmservice"), "origin 不应带路径: {}", origin);
            assert_eq!(label, "博世 MFC");
        }
        // 智谱系
        for (u, label) in [
            ("open.bigmodel.cn", "智谱 GLM"),
            ("https://open.bigmodel.cn/api/paas/v4/chat/completions", "智谱 GLM"),
            ("https://api.z.ai/api/anthropic", "智谱国际 GLM"),
        ] {
            let (k, _, label2) = detect_custom(u);
            assert_eq!(k, CustomKind::Zhipu, "url={}", u);
            assert_eq!(label2, label);
        }
        // 未识别
        let (k, _, _) = detect_custom("https://example.com/api/quota");
        assert_eq!(k, CustomKind::Unknown);
        let (k, _, _) = detect_custom("   ");
        assert_eq!(k, CustomKind::Unknown);
    }

    #[test]
    fn test_parse_reset_ms() {
        assert_eq!(parse_reset_ms(Some(&json!(1786112351998i64))), Some(1786112351998));
        assert_eq!(parse_reset_ms(Some(&json!(1786112351i64))), Some(1786112351000));
        assert_eq!(
            parse_reset_ms(Some(&json!("1786112351998"))),
            Some(1786112351998)
        );
        assert!(parse_reset_ms(Some(&json!("2026-12-01T00:00:00Z"))).is_some());
        assert_eq!(parse_reset_ms(Some(&json!(0))), None);
        assert_eq!(parse_reset_ms(None), None);
    }

    #[test]
    fn test_tier_limit() {
        assert_eq!(tier_limit("free"), 50.0);
        assert_eq!(tier_limit("pro"), 300.0);
        assert_eq!(tier_limit("pro+"), 1500.0);
        assert_eq!(tier_limit("business"), 300.0);
        assert_eq!(tier_limit("enterprise"), 1000.0);
        assert_eq!(tier_limit("whatever"), 300.0);
    }

    #[test]
    fn test_zhipu_parse() {
        // 样例来自 ClaudeBar zai design.md（lite 账号）
        let body: Value = serde_json::from_str(
            r#"{
            "code": 200,
            "data": {
                "limits": [
                    { "type": "CREDIT_LIMIT", "unit": 3, "number": 5, "usage": 2000,
                      "currentValue": 0, "remaining": 2000, "percentage": 0 },
                    { "type": "CREDIT_LIMIT", "unit": 6, "number": 1, "usage": 10000,
                      "currentValue": 2004, "remaining": 7995, "percentage": 20,
                      "nextResetTime": 1786112351998 }
                ],
                "level": "lite"
            },
            "success": true
        }"#,
        )
        .unwrap();
        let data = body.get("data").unwrap();
        let limits = data.get("limits").unwrap().as_array().unwrap();
        let mut windows: Vec<QuotaWindow> = Vec::new();
        for l in limits {
            let ltype = str_of(l.get("type")).to_ascii_uppercase();
            let unit = str_of(l.get("unit"));
            let kind = if ltype == "TIME_LIMIT" {
                "mcp"
            } else {
                match unit.as_str() {
                    "6" => "week",
                    "7" => "month",
                    _ => "5h",
                }
            };
            windows.push(QuotaWindow {
                kind: kind.to_string(),
                pct: f64_of(l.get("percentage")).clamp(0.0, 100.0),
                reset_at_ms: parse_reset_ms(l.get("nextResetTime")),
            });
        }
        assert_eq!(windows[0].kind, "5h");
        assert_eq!(windows[0].pct, 0.0);
        assert_eq!(windows[1].kind, "week");
        assert_eq!(windows[1].pct, 20.0);
        assert_eq!(windows[1].reset_at_ms, Some(1786112351998));
        assert_eq!(str_of(data.get("level")), "lite");
    }

    #[test]
    fn test_copilot_parse() {
        let body: Value = serde_json::from_str(
            r#"{
            "usageItems": [
                { "sku": "Copilot Premium Request", "model": "gpt-5.3", "grossQuantity": 100, "limit": 300 },
                { "sku": "Copilot Premium Request", "model": "claude-sonnet", "grossQuantity": 28, "limit": 300 },
                { "sku": "Copilot Chat", "model": "gpt-5.3", "grossQuantity": 999 }
            ],
            "timePeriod": { "year": "2026", "month": "09" }
        }"#,
        )
        .unwrap();
        let items = body.get("usageItems").unwrap().as_array().unwrap();
        let mut used = 0.0;
        let mut limit = 0.0;
        for it in items {
            let sku = str_of(it.get("sku")).to_ascii_lowercase();
            if sku.contains("premium") {
                used += f64_of(it.get("grossQuantity"));
                if limit <= 0.0 {
                    limit = opt_f64(it.get("limit")).unwrap_or(0.0);
                }
            }
        }
        assert_eq!(used, 128.0);
        assert_eq!(limit, 300.0);
        assert_eq!(str_of(body["timePeriod"].get("month")), "09");
    }

    #[test]
    fn test_mfc_parse() {
        let body: Value = serde_json::from_str(
            r#"{
            "code": 200,
            "msg": "ok",
            "data": {
                "startDate": "2026-09-01", "endDate": "2026-09-25",
                "promptTokens": 900000, "completionTokens": 300000, "totalTokens": 1200000,
                "cost": 12.34, "serviceFee": 1.2, "totalCost": 13.54,
                "periods": [],
                "details": [
                    { "model": "glm-4.7", "promptTokens": 500, "completionTokens": 300, "totalTokens": 800, "cost": 8.2 },
                    { "model": "gpt-5.3", "promptTokens": 100, "completionTokens": 50, "totalTokens": 150, "cost": 4.14 }
                ]
            }
        }"#,
        )
        .unwrap();
        let code = f64_of(body.get("code")) as u16;
        assert_eq!(code, 200);
        let m = parse_mfc_data(body.get("data"));
        assert_eq!(m.cost, 12.34);
        assert_eq!(m.total_tokens, 1200000.0);
        assert_eq!(m.models.len(), 2);
        assert_eq!(m.models[0].model, "glm-4.7"); // 按 cost 降序
        assert_eq!(m.models[0].value, 8.2);
    }

    #[test]
    fn test_usable_token() {
        assert!(usable_token("ghu_0123456789abcdefghijklmn"));
        assert!(usable_token("gho_0123456789abcdefghijklmn"));
        // Copilot 会话 token 必然 401，不能当 OAuth token 用
        assert!(!usable_token("tid=0123456789abcdefghijklmn"));
        assert!(!usable_token("short"));
        assert!(!usable_token("   "));
    }

    #[test]
    fn test_parse_reset_ms_bare_date() {
        // Copilot 的 quota_reset_date 是裸日期
        let ms = parse_reset_ms(Some(&json!("2026-10-01"))).unwrap();
        let dt = chrono::DateTime::from_timestamp_millis(ms).unwrap();
        assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-10-01");
        assert_eq!(parse_reset_ms(Some(&json!("not-a-date"))), None);
    }

    /// 样例取自本机实拉（org 席位 bosch-copilot，business 套餐）
    #[test]
    fn test_copilot_quota_parse() {
        let body: Value = serde_json::from_str(
            r#"{
            "copilot_plan": "business",
            "access_type_sku": "copilot_for_business_seat_quota",
            "organization_login_list": ["bosch-copilot"],
            "quota_reset_date_utc": "2026-10-01T00:00:00.000Z",
            "token_based_billing": true,
            "quota_snapshots": {
                "chat": { "entitlement": 0, "remaining": 0, "unlimited": true },
                "completions": { "entitlement": 0, "remaining": 0, "unlimited": true },
                "premium_interactions": {
                    "entitlement": 50000, "credits_used": 35697, "remaining": 14302,
                    "percent_remaining": 28.6, "overage_permitted": true, "unlimited": false
                }
            }
        }"#,
        )
        .unwrap();
        match copilot_quota_to_usage(&body, "pi 登录态") {
            UsageData::Copilot { used, limit, tier, credits, remaining, org, source, reset_at_ms, .. } => {
                assert_eq!(used, 35697.0);
                assert_eq!(limit, 50000.0);
                assert_eq!(remaining, 14302.0);
                assert!(credits, "新计量单位应是 AI Credits");
                assert_eq!(tier, "BUSINESS");
                assert_eq!(org, "bosch-copilot");
                assert_eq!(source, "pi 登录态");
                let dt = chrono::DateTime::from_timestamp_millis(reset_at_ms.unwrap()).unwrap();
                assert_eq!(dt.format("%Y-%m-%d").to_string(), "2026-10-01");
            }
            _ => panic!("应解析为 Copilot 快照"),
        }
    }

    /// 只有 remaining 没有 credits_used 时也要能反推 used（个别账号字段不全）
    #[test]
    fn test_copilot_quota_remaining_only() {
        let body: Value = serde_json::from_str(
            r#"{"copilot_plan":"individual","quota_snapshots":{"premium_interactions":{"entitlement":300,"remaining":120}}}"#,
        )
        .unwrap();
        match copilot_quota_to_usage(&body, "手填 Token") {
            UsageData::Copilot { used, limit, remaining, org, .. } => {
                assert_eq!(used, 180.0);
                assert_eq!(limit, 300.0);
                assert_eq!(remaining, 120.0);
                assert!(org.is_empty());
            }
            _ => panic!("应解析为 Copilot 快照"),
        }
    }

    /// 不限量 / 无快照 / 报错体：三种非正常返回都不能 panic
    #[test]
    fn test_copilot_quota_edge_cases() {
        let unlim: Value = serde_json::from_str(
            r#"{"copilot_plan":"business","quota_snapshots":{"premium_interactions":{"entitlement":0,"remaining":0,"unlimited":true}}}"#,
        )
        .unwrap();
        match copilot_quota_to_usage(&unlim, "s") {
            UsageData::Copilot { limit, .. } => assert_eq!(limit, 0.0, "不限量时 limit=0 交给前端显示"),
            _ => panic!("应解析为 Copilot 快照"),
        }

        let no_snap: Value = serde_json::from_str(r#"{"copilot_plan":"none"}"#).unwrap();
        assert!(matches!(copilot_quota_to_usage(&no_snap, "s"), UsageData::Unsupported { .. }));

        let err: Value = serde_json::from_str(r#"{"message":"Bad credentials"}"#).unwrap();
        assert!(matches!(copilot_quota_to_usage(&err, "s"), UsageData::Error { .. }));
    }

    #[test]
    fn test_next_month_reset() {
        let ms = next_month_first_utc_ms().unwrap();
        let now = Utc::now().timestamp_millis();
        assert!(ms > now, "重置点应在未来");
        let dt = chrono::DateTime::from_timestamp_millis(ms).unwrap();
        assert_eq!(dt.day(), 1);
    }
}
