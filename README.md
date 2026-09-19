---
title: gemini-buddy 使用说明
version: v0.2
author: 胡嘉
date: 2026-09-19
---

# gemini-buddy 使用说明

## 版本
| 版本 | 日期 | 作者 | 改动点 |
| --- | --- | --- | --- |
| v0.1 | 2026-09-19 | 胡嘉 | 首版：Option+G 呼出的 Gemini 桌面小窗（Electron 内嵌网页） |
| v0.2 | 2026-09-19 | 胡嘉 | 轻量重构：tkinter 浮窗 + Antigravity CLI（agy）问答，去掉 Electron；新增失焦自动隐藏 |

## 链接
- 无

---

## 是什么

按 `Option+G` 呼出一个置顶小浮窗，打字提问回车，后台调用 Antigravity CLI（`agy`，复用你的 Google 账号登录态，免 API key），回答直接显示在窗口里。点窗口外任意地方自动隐藏；回答还在生成时先不藏，答完弹在屏幕上（不抢键盘焦点）。

内存占用约 50MB，无 node_modules。

## 启动 / 停止

```bash
cd ~/Desktop/Working/2026-09_gemini-buddy
./start.sh        # 后台启动（日志: ~/.gemini-buddy.log）
./start.sh stop   # 停止
./start.sh log    # 看日志
```

## 快捷键与交互

| 操作 | 效果 |
| --- | --- |
| `Option+G` | 呼出 / 隐藏，呼出后直接打字 |
| `回车` | 发送问题，回答追加在窗口内（保留历史问答） |
| `Esc` | 关闭窗口 |
| 点窗口外 | 自动隐藏（回答生成中除外，答完再显示） |
| 窗口顶部 | 按住拖动位置 |

窗口出现在鼠标当前所在屏幕，居中偏上。每次提问是独立会话（无上下文）。

## 依赖（本机已全部就绪）

- `python3` + tkinter（系统自带）
- `pynput`（全局快捷键，已装 v1.8.1）
- `agy`（Antigravity CLI v1.2.3，`~/.local/bin/agy`，已 Google 账号登录）

pynput 快捷键需要辅助功能授权：若 `Option+G` 无反应，到「系统设置 → 隐私与安全性 → 辅助功能」给运行脚本的终端勾选。

## 配置（buddy.py 顶部）

| 常量 | 默认值 | 说明 |
| --- | --- | --- |
| `HOTKEY` | `'<alt>+g'` | 全局快捷键，pynput 格式 |
| `WIN_W / WIN_H` | 640 / 440 | 窗口尺寸 |
| `ANSWER_TIMEOUT` | 180 | 单次回答超时（秒） |

## 自动化测试

```bash
python3 buddy.py --test "2的10次方等于几？只回答数字"
```

建 UI → 发问题 → 等 agy 回答 → 打印回答区内容后退出。

## 历史

- v0.1 为 Electron 内嵌 gemini.google.com 网页方案（commit 7d4ffba），因占用高（~300MB）退役；git 历史可随时找回（`git checkout 7d4ffba -- main.js package.json`）。
