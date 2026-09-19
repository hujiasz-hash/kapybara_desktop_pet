---
title: gemini-buddy 使用说明
version: v0.9
author: 胡嘉
date: 2026-09-19
---

# gemini-buddy 使用说明

## 版本
| 版本 | 日期 | 作者 | 改动点 |
| --- | --- | --- | --- |
| v0.1 | 2026-09-19 | 胡嘉 | 首版：Option+G 呼出的 Gemini 桌面小窗（Electron 内嵌网页） |
| v0.2 | 2026-09-19 | 胡嘉 | 轻量重构：tkinter 浮窗 + Antigravity CLI（agy）问答，去掉 Electron |
| v0.3 | 2026-09-19 | 胡嘉 | 回归 Electron 但自绘透明对话框：解决 macOS 无边框窗口无法键盘输入的坑；流式回答、覆盖式问答；支持 Windows |
| v0.4 | 2026-09-19 | 胡嘉 | 多后端：新增 Pollinations 零注册零 key 免费网关自动兜底，Windows 无需安装/登录 agy |
| v0.5 | 2026-09-19 | 胡嘉 | agy 默认用低思考档模型（gemini-3.8-flash-low）加速回答；回答区支持粗体/行内码渲染 |
| v0.6 | 2026-09-19 | 胡嘉 | 默认后端改为 pollinations（实测 1~2 秒 vs agy 9~11 秒），agy 降为可选质量档（GB_BACKEND=agy） |
| v0.7 | 2026-09-19 | 胡嘉 | 等待回复增加"思考中"三点跳动动画 + 蓝点变黄脉冲提示，提交瞬间即有反馈 |
| v0.8 | 2026-09-19 | 胡嘉 | 界面改为卡皮巴拉主题：右上角 SVG 卡皮巴拉（瞳孔跟随鼠标、随机眨眼/咀嚼/歪头/小跳/瞌睡、点击会开心跳），面板更透明；修连续提问不覆盖的 bug |
| v0.9 | 2026-09-19 | 胡嘉 | 卡皮巴拉改为像素风（agy 出像素稿 + canvas 帧渲染，参考 Claude Code 桌宠风格）：20x16 像素 5x 放大，idle 摇橘子/眨眼/跳跃/瞌睡帧动画，瞳孔像素级跟随鼠标，点击互动 |

## 链接
- 无

---

## 是什么

按 `Option+G`（Windows: `Alt+G`）呼出一个半透明对话框，打字回车提问，后台调用 Antigravity CLI（`agy`，复用 Google 账号登录态，免 API key），回答**流式**显示在窗口里，**每次提问覆盖上一条**。点窗口外自动隐藏。

**后端（自动选择，也可用环境变量 `GB_BACKEND=agy|pollinations` 强制）**：

| 后端 | 选择方式 | 特点 |
| --- | --- | --- |
| `pollinations` | **默认** | 零注册零 key，实测 1~2 秒；GPT-4o-mini 级模型；问题会经过第三方服务，别问敏感内容 |
| `agy` | `GB_BACKEND=agy`（需装 CLI 并登录） | 9~11 秒，质量更好 |

技术底座：Electron（透明无边框窗 + 系统级全局快捷键，无需辅助功能授权）。

## macOS 启动 / 停止

```bash
cd ~/Desktop/Working/2026-09_gemini-buddy
./start.sh        # 后台启动（日志: ~/.gemini-buddy.log）
./start.sh stop   # 停止
./start.sh log    # 看日志
```

## Windows 部署

1. 安装 Node.js LTS（nodejs.org 下载即可）
2. 把整个项目文件夹拷到 Windows 机器，在该目录下：

（可选）想要更好的回答质量：安装 Antigravity CLI 并登录（https://antigravity.google/docs/cli/install ），装了自动切换到 agy 后端。

```bat
npm install
npm start
```

或直接双击 `start.bat`。之后 `Alt+G` 呼出。

## 快捷键与交互

| 操作 | 效果 |
| --- | --- |
| `Option+G` / `Alt+G` | 呼出 / 隐藏，呼出后光标已在输入框 |
| `Enter` | 发送问题，回答流式显示，**覆盖上一条** |
| `Esc` | 关闭窗口 |
| 点窗口外 | 自动隐藏（回答生成中不隐藏，答完停留 4 秒） |
| 点击左下角蓝点 | 中止当前回答 |
| 拖动顶部栏 | 移动窗口 |

窗口出现在鼠标当前所在屏幕，居中偏上。每次提问独立会话（无上下文）。

## 配置（main.js 顶部）

| 常量 | 默认值 | 说明 |
| --- | --- | --- |
| `HOTKEY` | mac `Option+G` / win `Alt+G` | 全局快捷键 |
| `WIN_W / WIN_H` | 640 / 400 | 窗口尺寸 |
| `ANSWER_TIMEOUT_MS` | 180000 | 单次回答超时 |
| `AGY_MODEL` | `gemini-3.8-flash-low` | agy 思考档位，low 最快；置空用 agy 默认；环境变量 `GB_AGY_MODEL` 可覆盖 |

## 网络

默认走系统代理。若需强制代理：

```bash
GB_PROXY=http://127.0.0.1:7898 npm start
```

## 自动化测试

```bash
GB_TEST_ASK="1+1等于几" npm start   # 自动呼出、提问、打印回答后退出
GB_DEBUG_SHOT=/tmp/shot.png npm start   # 自动呼出、截图后退出
```

## 历史

- v0.1 Electron 内嵌网页方案（commit 7d4ffba）：重（~300MB）退役
- v0.2 tkinter + agy（a2d7a75）：macOS 无边框窗口不能成为 key window（无法键盘输入），PyObjC swizzle 补丁路线过深，放弃；Windows 上 tkinter 其实可用，但为统一技术栈放弃
- v0.3 现方案：Electron 自绘 UI，快捷键/透明/输入全部原生支持，跨平台一致
