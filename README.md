---
title: gemini-buddy 使用说明
version: v0.1
author: 胡嘉
date: 2026-09-19
---

# gemini-buddy 使用说明

## 版本
| 版本 | 日期 | 作者 | 改动点 |
| --- | --- | --- | --- |
| v0.1 | 2026-09-19 | 胡嘉 | 首版：Option+G 呼出的 Gemini 桌面快速问答小窗（Electron） |

## 链接
- 无

---

## 是什么

按 `Option+G` 呼出一个置顶小窗，里面就是 Gemini 网页。直接打字提问，回答显示在窗口里；再按 `Option+G` 或 `Esc` 隐藏。登录态持久保存，登录一次以后免登录。

## 启动

```bash
cd ~/Desktop/Working/2026-09_gemini-buddy
npm start
```

启动后没有窗口、不占 Dock，常驻后台，随时 `Option+G` 呼出。

## 首次使用（只需一次）

1. `Option+G` 呼出窗口，显示 Gemini 落地页
2. 点右上角 Sign in，登录你的 Google 账号（登录态存在本应用独立 session，不影响 Chrome）
3. 登录后自动进入对话页，以后呼出直接可用

## 快捷键与交互

| 操作 | 效果 |
| --- | --- |
| `Option+G` | 呼出 / 隐藏窗口，呼出后焦点自动落到 Gemini 输入框 |
| `Esc` | 输入框为空时隐藏窗口；有内容时放行给页面（如停止生成） |
| 窗口顶部左侧 | 按住可拖动窗口位置（记忆在本次会话内） |
| `Cmd+Q`（窗口聚焦时） | 退出应用 |

窗口出现在鼠标当前所在的屏幕，居中偏上。

## 配置（都在 main.js 顶部）

| 常量 | 默认值 | 说明 |
| --- | --- | --- |
| `TOGGLE_ACCELERATOR` | `Option+G` | 呼出快捷键，被占用时改这里（如 `Cmd+Alt+G`） |
| `WIN_WIDTH` / `WIN_HEIGHT` | 760 / 680 | 窗口尺寸 |

## 网络

默认走系统代理。若需强制代理：

```bash
GB_PROXY=http://127.0.0.1:7898 npm start
```

## 技术栈

- Electron：全局快捷键 + 无边框置顶窗口 + `persist:gemini` 持久 session
- 不调用 API key，直接内嵌 gemini.google.com 网页
