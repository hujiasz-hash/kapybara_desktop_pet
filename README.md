---
title: kapybara-buddy 使用说明
version: v4.1
author: 胡嘉
date: 2026-09-21
---

# kapybara-buddy 使用说明

## 版本
| 版本 | 日期 | 作者 | 改动点 |
| --- | --- | --- | --- |
| v0.1 | 2026-09-19 | 胡嘉 | 首版：Option+G 呼出的 AI 桌面小窗（Electron 内嵌网页） |
| v0.2 | 2026-09-19 | 胡嘉 | 轻量重构：tkinter 浮窗 + Antigravity CLI（agy）问答，去掉 Electron |
| v0.3 | 2026-09-19 | 胡嘉 | 回归 Electron 但自绘透明对话框：解决 macOS 无边框窗口无法键盘输入的坑；流式回答、覆盖式问答；支持 Windows |
| v0.4 | 2026-09-19 | 胡嘉 | 多后端：新增 Pollinations 零注册零 key 免费网关自动兜底，Windows 无需安装/登录 agy |
| v0.5 | 2026-09-19 | 胡嘉 | agy 默认用低思考档模型（gemini-3.8-flash-low）加速回答；回答区支持粗体/行内码渲染 |
| v0.6 | 2026-09-19 | 胡嘉 | 默认后端改为 pollinations（实测 1~2 秒 vs agy 9~11 秒），agy 降为可选质量档（KB_BACKEND=agy） |
| v0.7 | 2026-09-19 | 胡嘉 | 等待回复增加"思考中"三点跳动动画 + 蓝点变黄脉冲提示，提交瞬间即有反馈 |
| v0.8 | 2026-09-19 | 胡嘉 | 界面改为卡皮巴拉主题：右上角 SVG 卡皮巴拉（瞳孔跟随鼠标、随机眨眼/咀嚼/歪头/小跳/瞌睡、点击会开心跳），面板更透明；修连续提问不覆盖的 bug |
| v0.9 | 2026-09-19 | 胡嘉 | 卡皮巴拉改为像素风（agy 出像素稿 + canvas 帧渲染，参考 Claude Code 桌宠风格）：20x16 像素 5x 放大，idle 摇橘子/眨眼/跳跃/瞌睡帧动画，瞳孔像素级跟随鼠标，点击互动 |
| v1.0 | 2026-09-19 | 胡嘉 | 重画像素稿强化卡皮巴拉特征（前突方鼻吻、宽距小眼、桶状身），对外定稿 |
| v1.1 | 2026-09-19 | 胡嘉 | 架构升级为双窗口：卡皮巴拉桌面常驻（官方素材 6 动作帧、朝向跟随鼠标、随机踱步打盹、可拖动、点击弹问答），问答窗贴宠物弹出并改像素风 UI |
| v1.2 | 2026-09-19 | 胡嘉 | 动作库扩到 18 帧（素材全量归一 420x420）：新增欢呼/放空/打工/喝咖啡/泡温泉/打瞌睡/顶橘/快跑/捧爱心/嚼三叶草/吃橘子；随机行为带权重和连招（打瞌睡→睡死、喝咖啡→打工）；事件动画：答完欢呼、开问答送爱心、拖动时"被提溜" |
| v1.3 | 2026-09-19 | 胡嘉 | 修拖动到屏幕边缘鼠标与宠物位置漂移：拖动改绝对坐标计算（按下时窗口位置+鼠标净位移），不再用增量累加 |
| v1.4 | 2026-09-19 | 胡嘉 | 修拖到台前调度图标区宠物闪烁：拖动范围 clamp 在工作区内 6px 边距 |
| v1.5 | 2026-09-19 | 胡嘉 | 睡觉改为鼠标空闲驱动：静止 15s 打瞌睡、30s 趴下睡死、一动就醒来欢呼；更新 picked_up 素材；活跃期随机行为池（踱步/吃草/吃橘子/放空/顶橘/快跑/咖啡→打工连招/温泉彩蛋） |
| v1.6 | 2026-09-19 | 胡嘉 | 修 v1.4 拖动基准污染（move 回声吸收致漂移）改纯绝对坐标；修 start.sh 进程匹配+清重复 IPC handler；三边缘零漂移回归 |
| v1.7 | 2026-09-19 | 胡嘉 | 行为节奏调快（间隔 2~4.5s，占空比约 50%）；picked_up 素材更新归一；加状态导出与 KB_PET_DEBUG 调试通道 |
| v1.8 | 2026-09-19 | 胡嘉 | 修拖动卡死在"被提溜"帧：小窗口快速拖动时鼠标冲出窗口导致 mouseup 丢失——加 setPointerCapture + e.buttons=0 兜底强制恢复 |
| v1.9 | 2026-09-19 | 胡嘉 | 拖动恢复三层保险：capture（窗外 up 送达）+ buttons 兜底 + 主进程 drag-lost（光标窗外 0.9s 无拖动 IPC 强制恢复，不依赖窗口事件）；endDrag 无条件重置 mode |
| v2.0 | 2026-09-19 | 胡嘉 | 修真凶：frameName() 里 `if (dragged)` 漏调用括号——dragged 是函数永远真值，导致永远显示提溜帧（状态机其实一直正常）；像素对比验证动画恢复 |
| v2.1 | 2026-09-19 | 胡嘉 | 修空帧：行为播放中被拖动打断→松开后 mode='idle' 但 behaviorHoldUntil 未过期→返回不存在的帧键→img 空白；三重修复（分支排除 idle / endDrag 作废保持期 / FRAME_FILES 补 idle 防御键）+ onerror 探针，5 轮拖动打断复测零失败 |
| v2.2 | 2026-09-19 | 胡嘉 | 修右侧台前调度区拖动闪烁：拖动中动态学习系统真实边界（以最后稳定位置为界，弹回落点不可用），学到的边界 clamp 收紧不再进入禁区；每次拖动重学，台前调度开关自适应；实测学到 1258 零抖动 |
| v2.3 | 2026-09-19 | 胡嘉 | 修 v2.2 钉死回归：快速拖动时 setPosition 应用延迟被误判为系统弹回→误学边界（minX=maxX 钉死 x 轴）；改确认式学习（不符后延时 150ms 复核，持续拒绝才算真弹回）+ 边界学习防钉死校验（min/max 至少留 200px）；复测右顶界→快速左甩 380px 正常 |
| v2.4 | 2026-09-19 | 胡嘉 | 修首次拖入右侧"闪入再弹回"：启动时利用初始位置(1382)被系统弹回到真实边界的现成事件预学边界，第一帧起 clamp 生效；边界进程级持久（不再每次拖动重学重闪）；台前调度启动后才开启的场景由运行中确认式学习兜底（首次拖入会闪一次） |
| v2.5 | 2026-09-19 | 胡嘉 | 跨平台修复：调试日志路径 /tmp 硬编码改为 os.tmpdir()（Windows 无 /tmp 会崩） |
| v2.6 | 2026-09-19 | 胡嘉 | 右键宠物随机换动画（mousedown 入口）；清理 pyc 遗留、完善 .gitignore |
| v2.7 | 2026-09-19 | 胡嘉 | 修右键双 bug：mouseup 未过滤按键致右键松开误触 endDrag（轻点看似无响应、按压松开回旧帧）；右键入口改 mousedown(button=2)，contextmenu 只屏蔽菜单 |
| v2.8 | 2026-09-19 | 胡嘉 | 右键池补欢呼+捧爱心（原本只有事件动画可见）；weightedPick 支持自定义池，30 次连点全 11 种动作轮到 |
| v2.9 | 2026-09-19 | 胡嘉 | 更名 gemini-buddy → kapybara-buddy：文件夹/README/启动脚本/日志名(~/.kapybara-buddy.log)/调试环境变量前缀 GB_→KB_；start.sh pkill 模式同步新路径；模型名 gemini-3.8-flash-low 为 agy 参数保留 |
| v2.10 | 2026-09-19 | 胡嘉 | 修白边：朝向四帧（idle_front/look_left/look_right/look_up）贴边白描边 ~950px/帧 → 改为素材统一的黑描边；贴边白点复检归零、与无瑕疵帧结构一致 |
| v2.11 | 2026-09-19 | 胡嘉 | 修透明眼睛：朝向四帧眼区内部透明孔洞（16~290px）填白——原先透出桌面背景；四帧复检孔洞归零 |
| v3.0 | 2026-09-19 | 胡嘉 | pi 集成第一步：新增 pi 扩展把 session 生命周期推给桌宠（打工/欢呼/捧爱心/睡觉），本地回环 HTTP 通道（127.0.0.1:17898）；顺修事件动画哑火 bug（主进程发 'cheering' 而渲染层判 'cheer'，名字对不上）；加事件优先级——pi 秒答完时"干完欢呼"能打断"打工" |
| v3.1 | 2026-09-19 | 胡嘉 | 状态动效：素材是静态帧，换图肉眼几乎看不出变化（实测反馈"没反应"）——按状态加 transform 动画（打字抖动/欢呼跳跃/睡觉呼吸/走路摇摆/被提溜晃 等 14 种）；欢呼时长 1.8s→3.5s 免错过 |
| v3.2 | 2026-09-19 | 胡嘉 | CSS 平滑动效改真逐帧动画（实测反馈"假且晕"）：离线用整数像素位移生成 34 个动画帧（13 状态 × 2~4 帧，ui/frames/anim/），按帧率硬切换，无亚像素插值；幅度按显示尺寸 1~7px 控制 |
| v3.3 | 2026-09-19 | 胡嘉 | 换全套 17 动作 × 3 帧真素材（敲键盘/被拎/咖啡/欢呼/发呆/温泉/漫步/快跑/瞌睡/安睡/顶橘/嚼草/爱心/吃橘/生气/托腮思考/灵光一现），帧序与节奏照抄作者给的 GIF（typing 是 `1,2,3,2` 乒乓、clover 是 `1,2,1,2,3`）；旧朝向四帧下移 19px 对齐新素材脚底线，旧 18 帧归档 legacy_18/；hook 事件扩到 8 类（工具成功→灵光一现、失败→气到冒烟、压缩→发呆）；修两个真 bug：①长保持期把紧随其后的事件全吃掉（session_start→agent_start 只隔零点几秒，卡皮巴拉一直捧爱心不进思考/打工）②自娱行为每次刷新 lastActivity 致睡意永远触发不了（旧版就有） |
| v3.4 | 2026-09-19 | 胡嘉 | 节奏放慢（实测反馈"换得太勤、眼花"）：每帧时长统一 ×1.4（快跑 ×1.15 保住冲刺感）；单次自娱动作从 3~4s 拉长到 9~14s（动作内循环 4~7 轮）；动作之间空 7~14s（原先 2~4.5s）；事件保持期按轮数同步拉长 |
| v3.5 | 2026-09-19 | 胡嘉 | 鼠标长时间不动就一直打瞌睡（用户要求）：去掉"静止 30s 转入趴下安睡"那一档，15s 后保持打瞌睡直到鼠标动；"趴下安睡"动作留给 pi 的 session_shutdown |
| v3.6 | 2026-09-19 | 胡嘉 | 行为重排：①朝向优先于自娱——鼠标进到桌宠周边 300px 内就只看着鼠标、不演自娱（离得远才自己玩）②打瞌睡阈值 15s→30s，"惊醒"帧（眼睛圆睁+汗）从打瞌睡循环里拿掉，只有鼠标动才播 ③"趴下安睡"从 pi 会话关闭挪到睡眠待机：静止 30s 打瞌睡 → 再 30s 趴下安睡 ④pi session_shutdown 不再触发动画 |
| v3.7 | 2026-09-19 | 胡嘉 | 朝向触发圈从 300px 缩到 100px（约贴到跟前才看鼠标）；顺手补齐 README 里 v3.6 漏改的行为描述 |## 链接
| v3.8 | 2026-09-19 | 胡嘉 | 朝向触发圈 100px→50px（圈已小于方向判定阈值，凑近时基本只显示正面；远处待机时的朝向跟随不受影响） |- 无
| v3.9 | 2026-09-19 | 胡嘉 | 方向判定阈值 55/75px→30px（原阈值比 50px 触发圈还大，凑近时只能显示正面；现在鼠标压在它身上左右移动，头会跟着转） |
| v3.10 | 2026-09-19 | 胡嘉 | 方向判定阈值 30px→20px（鼠标在它身上偏 20px 以上才转头，更容易停在正面） |
| v4.0 | 2026-09-21 | 胡嘉 | 去 Electron 重构：Tauri v2 + 系统 WebView（WKWebView/WebView2），装包从 289MB node_modules 变 13MB 单二进制；前端 HTML/PNG 帧零改动（window.buddy 桥接层用注入 shim 等价替换）；pi 事件通道/热键/拖动边界学习/双后端问答全部保留；移除 KB_PROXY（原只影响渲染进程）与 KB_DEBUG_SHOT（无对应截图 API）；详见 src-tauri/ |---
| v4.1 | 2026-09-21 | 胡嘉 | 支持 Antigravity 与 Claude Code 官方生命周期 Hook；重构事件通道为多 Agent 会话聚合引擎（解决并发冲突、早退误睡与僵尸会话自愈）；提供 agents/ 一键安装脚本 |
| v5.0 | 2026-09-25 | 胡嘉 | 去问答，转订阅用量面板：①左键点桌宠改弹订阅配置（Copilot 默认 / 智谱中国只填 Key / 自定义 URL+Key，URL 自适应识别博世 MFC、bigmodel.cn、api.z.ai）②鼠标悬停桌宠弹用量面板（Premium 月额度、智谱 5h/周/月 窗口、MFC 月/今日费用+tokens，带重置倒计时与模型 Top5）③删除问答链路（answer.rs / index.html / KB_TEST_ASK），Option+G 改呼出配置面板；用量后台 5min 轮询（KB_USAGE_POLL_SEC 可调）配置存 app_data_dir/usage.json |
| v5.1 | 2026-09-25 | 胡嘉 | 修「鼠标放在桌宠上没有悬浮显示用量」：悬停触发从渲染层 DOM 事件搬到主进程——桌宠窗是 `focusable(false)`（设计上不许抢焦点），非 key 窗口的 WebView 命中测试不可靠：光标已经在宠物身上时窗口才出现、或窗口拿不到 key，就收不到 `mouseenter`；而 `mouseleave` 同样不来，enter/leave 配不上对即永久哑火（实测日志只有 1 次 hover-enter、0 次 show=true）。改由主进程按**全局光标**判定（那 120ms 轮询本来就为转头而跑）：「桌宠矩形 ∪ 面板矩形」进圈 350ms 弹出、出圈交给面板的 600ms 宽限（光标从桌宠挪到面板上不误判移开）；拖动中/松手 800ms 内、配置面板开着时让位（左键点桌宠 = 进配置，别在它脸上再叠一张）。pet.html 不再挂鼠标事件，pet shim 去掉 `hoverUsage`，`usage_hover` 命令删除；新增 `KB_USAGE_DEBUG` 逐 tick 打印光标与桌宠矩形 |
| v5.2 | 2026-09-25 | 胡嘉 | Copilot 卡片改显示真实 **AI Credits**：2026-06-01 起 Copilot 转按量计费，旧的 `premium_request/usage` 账单接口对**组织分配的席位**（如 `bosch-copilot` 发的 Business 席位）返回 200 但 `usageItems` 为空 —— 卡片永远显示假的 `0/300`。改为走 Copilot 客户端同款内部接口 `GET api.github.com/copilot_internal/user`，读 `quota_snapshots.premium_interactions`（`entitlement` / `credits_used` / `remaining` / `percent_remaining`）。该接口**只认 OAuth token**（`gho_` / `ghu_`）：细粒度 PAT 401、Copilot 会话 token（`tid=…`）也 401。凭据按 **手填 token → pi 登录态（`~/.pi/agent/auth.json` 的 `github-copilot.refresh`）→ macOS 钥匙串 `copilot-cli`** 顺序自动发现，全找不到才退回旧账单接口；配置面板新增「凭据来源 + Token」两行，卡片加 `bosch-copilot 席位 · pi 登录态` 来源行与千分位数字 |

| v5.3 | 2026-09-25 | 胡嘉 | 用量面板转极简：①**只显示已配置的账号**，没配置的卡片不占位（全空时给一行提示）②所有卡片**统一高度**（`.acct { min-height:64px }` 固定三段：头部 / 标尺 / 说明行），再也不会一张三行一张七行③Copilot 去掉「凭据来源」那行（`pi 登录态` 之类），只留 `剩 14,302（29%）· 4天后重置 · bosch-copilot`；模型 Top5 一并删掉④博世 MFC 只显示总金额 `¥102.55`，不再列今日/模型明细⑤窗口高度跟着卡片数收拢：渲染层算好内容高度上报 `usage_panel_height`，主进程缓存后**下次弹出前**改窗口尺寸（显示中只允许变高，变矮会把光标甩出悬停保持区）⑥`cursor_in_hover_zone` 改用窗口实际尺寸，不再拿写死的 `USAGE_H` 判命中 |

## 链接
- 无

---

## 是什么

一只卡皮巴拉桌宠常驻桌面，**鼠标悬停它就能看你所有 AI 订阅账号的用量**（GitHub Copilot / 智谱 GLM Coding Plan / 博世 MFC），**左键点击进入极简订阅配置**；右键随机换动画、拖动随身带；本地事件通道让 Antigravity / Claude Code / pi 的生命周期驱动它的动作。

**订阅用量（v5.0 / Copilot 计量 v5.2）**

| 供应商 | 配置 | 查询内容 | 接口 |
| --- | --- | --- | --- |
| GitHub Copilot（默认） | 凭据来源（自动 / 手填 Token / PAT） | **AI Credits** 月额度：已用 / 上限 / 剩余（%）/ 重置倒计时 + 席位组织 | 首选 `api.github.com/copilot_internal/user` → `quota_snapshots`；兜底 `api.github.com/users/{u}/settings/billing/premium_request/usage`（旧 Premium 请求，个人订阅） |
| 智谱中国 | **只填 API Key**，无需 URL | GLM Coding Plan 5h / 本周 / 本月窗口 + 重置时间 | `open.bigmodel.cn/api/monitor/usage/quota/limit` |
| 自定义 | URL + Key | 博世 MFC：月/今日费用+tokens+模型 Top5；URL 若是智谱系则同智谱查询 | `aigc.bosch.com.cn/llmservice/api/v1/client/usage` |

自定义 URL 自适应：裸域名 / 完整接口地址 / 带不带 `https://` 都能识别；识别不了仅保存配置并在面板标注。轮询默认 5 分钟（`KB_USAGE_POLL_SEC` 可调），配置明文存于 `app_data_dir/usage.json`（仅本机）。

## 快捷键与交互

| 操作 | 效果 |
| --- | --- |
| 鼠标悬停桌宠（350ms） | 弹出订阅用量面板，移开即隐 |
| 左键点击桌宠 | 弹出订阅配置面板（点外面/Esc 关闭） |
| `Option+G` / `Alt+G` | 呼出 / 隐藏配置面板 |
| 左键拖动 / 右键 | 被拎悬空扑腾 / 随机换一个自娱动作 |
| 点击用量面板「刷新」 | 立即重新查询所有启用账号 |

## macOS 启动 / 停止

```bash
cd ~/Desktop/Working/2026-09_kapybara-buddy
./start.sh        # 后台启动（日志: ~/.kapybara-buddy.log）
./start.sh stop   # 停止
./start.sh log    # 看日志
./start.sh build  # 重新编译 release
```

## Windows 部署

1. 安装 Rust 工具链（rustup.rs）
2. 把整个项目文件夹拷到 Windows 机器，在该目录下双击 `start.bat`（首次会自动编译，产物在 `%LOCALAPPDATA%\kapybara-buddy-target`）。之后 `Alt+G` 呼出。

## 素材与形态

**素材**：17 个动作 × 3 帧（420×420 透明 PNG，`app/frames/`），另有朝向四帧（`idle_front/left/right/up.png`，沿用旧素材、已下移 19px 与新帧对齐脚底线）。每个动作的帧序和节奏**照抄作者给的 GIF**——不是简单 `1→2→3` 循环：`typing` 是 `[1,2,3,2]` 乒乓、`clover` 是 `[1,2,1,2,3]`、`sleep`/`heart`/`angry_zen` 是 `[1,2,3,1]`；每帧时长也可能不同（`nodding` 是 800/1000/800ms）。所以渲染层用递归 `setTimeout` 而非固定 `setInterval`。旧的 18 帧素材在 `app/frames/legacy_18/`。

**形态（无 hook，桌宠自己活着）**

| 形态 | 触发 | 动作 |
| --- | --- | --- |
| 待机 | 鼠标在桌宠周边 **50px** 内（约压在它身上），或它在待机空档 | 朝向四帧跟随鼠标 |
| 打瞌睡 | 鼠标静止 15s | 坐立瞌睡惊醒（**一直保持到鼠标动**） |
| 自娱 | 活跃时随机 | 漫步 / 快跑 / 嚼三叶草 / 吃橘子 / 咖啡 / 顶橘 / 发呆 / 泡温泉（彩蛋权重低） |

**节奏**：朝向跟随是即时响应（不动）；其余动画放慢一档——每帧时长 ×1.4（快跑 ×1.15），单个自娱动作演 9~14 秒（动作内循环 4~7 轮），动作之间空 7~14 秒。

**朝向优先于自娱**：鼠标进入桌宠周边 `NEAR_R`（50px）内 → 它只看着鼠标，不演自娱动作；鼠标离开这个圈，才继续自己玩。这样"凑近看它"和"它在旁边自己活着"是两种感觉。

**睡眠待机分两段**：鼠标静止 30s → 打瞌睡（点头循环，**不含**"惊醒"帧）；再不动 30s（共 60s）→ 趴下安睡。鼠标一动立刻播完整版"坐立瞌睡惊醒"（那帧眼睛圆睁+冒汗只在此时出现），然后回正常。
| 交互 | 左键拖动 / 右键 | 被拎悬空扑腾 / 随机换一个自娱动作 |

**形态（有 hook，Agent 驱动）**：见下节。`thinking/typing/inspiration/angry_zen/heart/cheering` 六个动作专供 AI Agent 驱动，不会出现在自娱池里——避免"没干活却在敲键盘"的假动作。

## AI Agent 联动（Antigravity / Claude Code / pi）

桌宠提供本地统一的 Agent 事件通道（`http://127.0.0.1:17898/agent-event`），支持将 **Antigravity**、**Claude Code** 和 **pi** 的生命周期事件直接推给卡皮巴拉：

| 动作语义 | 卡皮巴拉动效 | 动作类型 | Antigravity 事件 | Claude Code 事件 | pi 事件 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **会话启动** | `heart` (捧爱心 3.2s) | 瞬时 (Pri 2) | （IDE 打开/注入） | `SessionStart` | `session_start` |
| **深度思考** | `thinking` (托腮思考) | 工作态 (Pri 2) | `PreInvocation` | `UserPromptSubmit` | `agent_start` |
| **动手打工** | `typing` (敲键盘) | 工作态 (Pri 1) | `PreToolUse` | `PreToolUse` | `tool_execution_start` |
| **工具成功** | `inspiration` (电灯泡 2.5s) | 瞬时 (Pri 2) | `PostToolUse` (无 error) | `PostToolUse` | `tool_execution_end` (ok: true) |
| **工具失败** | `angry_zen` (气到冒烟 3.5s) | 瞬时 (Pri 3) | `PostToolUse` (含 error) | `PostToolUseFailure` | `tool_execution_end` (ok: false) |
| **上下文压缩** | `spacing_out` (灵魂发呆 3.0s) | 瞬时 (Pri 2) | — | `PreCompact` | `session_compact` |
| **任务搞定** | `cheering` (欢呼雀跃 3.2s) | 瞬时 (Pri 3) | `Stop` | `Stop` | `agent_settled` |
| **会话结束** | `sleep` (趴下安睡 3.0s) | 瞬时 (Pri 2) | （进程销毁） | `SessionEnd` | `session_shutdown` |

### 一键安装与配置 Hook

项目内置自动化安装与管理脚本，自动备份现有配置并在独立命名空间注入，零冲突随时还原：

```bash
# 查看当前 Antigravity / Claude Code Hook 安装状态
./agents/install-hooks.sh status

# 一键安装（自动备份 ~/.gemini/config/hooks.json 和 ~/.claude/settings.json）
./agents/install-hooks.sh install

# 随时一键完整卸载还原
./agents/install-hooks.sh uninstall
```

**pi 扩展安装**：
```bash
ln -sf ~/Desktop/Working/2026-09_kapybara-buddy/pi-extension/kapybara-bridge.ts ~/.pi/agent/extensions/kapybara-bridge.ts
```

### 多 Agent 并发与冲突仲裁机制

当用户同时开启多个终端或项目，并发运行 Antigravity、Claude Code 或 pi 时，系统通过会话聚合引擎（`SessionTracker`）统一仲裁：
1. **防早退误休眠**：若 Agent A 正在跑长时间工具（`typing`），Agent B 回答完触发了欢呼（`cheering`），桌宠为 Agent B 短暂欢呼 3 秒后，会自动回退至 Agent A 的 `typing` 打工态，绝不会意外躺平睡觉；
2. **动手优先于动脑**：任意活跃 Agent 在跑工具（`typing`）能级高于思考（`thinking`）；
3. **防僵尸卡死（TTL 心跳租约）**：若在终端对运行中的 Agent 执行 `Ctrl+C` 强退，会话引擎在 120s~180s 无新事件后自动清理僵尸会话，桌宠自愈恢复正常待机；
4. **非阻塞安全保障**：Hook 脚本采用纯 `/bin/sh` + 异步后台 `curl`，耗时小于 2ms，桌宠未启动时静默跳过，绝不阻塞开发终端。

## 架构（v4.1 起）

```
app/                     前端（打包进二进制）
  pet.html               桌宠渲染层（帧动画/多Agent状态机/拖动交互；悬停触发在主进程，见 main.rs hover_tick）
  config.html            订阅配置面板（左键呼出）
  usage.html             悬浮用量面板（悬停呼出）
  frames/                PNG 帧素材
src-tauri/               Rust 主进程（Tauri v2）
  src/main.rs            窗口创建/热键/光标轮询/生命周期
  src/usage.rs           订阅用量：三供应商查询/URL 适配/轮询/面板窗口管理
  src/agent_event.rs     多 Agent 事件通道与会话聚合仲裁引擎（:17898）+ 事件动画辅助
  src/pievent.rs         兼容旧版引用的向后兼容层
  src/drag.rs            拖动绝对坐标 + 台前调度边界学习
  src/bridge.rs          window.buddy 注入 shim
  src/commands.rs        IPC 命令
  src/state.rs           共享状态（拖动/边界/用量）
agents/                  Hook 脚本与集成工具
  antigravity/           Antigravity hook 脚本与 hooks.json
  claude/                Claude Code hook 脚本与配置片段
  install-hooks.sh       一键安装/卸载/状态管理脚本
pi-extension/            pi 扩展
```

## 配置

| 环境变量 | 默认 | 说明 |
| --- | --- | --- |
| `KB_USAGE_POLL_SEC` | `300` | 订阅用量后台轮询间隔（秒，60~86400） |
| `KB_PET_DEBUG` | 未开 | 开启后桌宠状态每 300ms 上报到 `$TMPDIR/pet-debug.log` |
| `KB_FAST` | 未开 | 加速所有时间阈值（测试用） |
| `KB_USAGE_DEBUG` | 未开 | 逐 tick 打印光标位置、悬停命中/离开与桌宠矩形（排查"悬停不弹用量面板"用） |

注：v5.0 移除 `KB_BACKEND` / `KB_AGY_MODEL` / `KB_TEST_ASK` / `KB_TEST_ASK2`（问答功能已删）。窗口尺寸/热键等常量在 `src-tauri/src/main.rs` 与 `geom.rs` 顶部。

## 本机构建注意（macOS）

这台机器的 CLT 是 2020 年的 Xcode 12（clang 12 / ld64-609），其链接产物会被 macOS 26 内核 AMFI 拒绝（rustc 加载 proc-macro 即被 SIGKILL "Code Signature Invalid"）。`src-tauri/.cargo/config.toml` 已改用 Homebrew LLVM/LLD 链接：

```bash
brew install llvm lld
```

另外编译产物目录已从项目内（~/Desktop，受 iCloud 同步管理）移到 `~/Library/Caches/kapybara-buddy-target`，避免 fileproviderd 动新鲜 dylib 的元数据触发同款误杀。

## 网络

用量查询（Copilot/智谱/MFC）与 Agent 事件通道均直连；需要代理时设系统代理即可（reqwest 会读取系统代理配置）。

## 自动化测试

```bash
cd src-tauri
cargo test                                  # URL 适配 / 三供应商解析 / 重置时间单测
KB_PET_DEBUG=1 cargo run                    # 桌宠状态 300ms 上报，tail $TMPDIR/pet-debug.log
```

## 历史

- v0.1 Electron 内嵌网页方案（commit 7d4ffba）：重（~300MB）退役
- v0.2 tkinter + agy（a2d7a75）：macOS 无边框窗口不能成为 key window（无法键盘输入），PyObjC swizzle 补丁路线过深，放弃；Windows 上 tkinter 其实可用，但为统一技术栈放弃
- v0.3 现方案：Electron 自绘 UI，快捷键/透明/输入全部原生支持，跨平台一致
- v4.0 去 Electron：Tauri v2（系统 WebView + Rust 主进程），前端零改动平移；仓库从 289MB node_modules 变成 13MB 单二进制
