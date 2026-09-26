#!/usr/bin/env bash
set -e

# ==============================================================================
# kapybara-buddy Agent Hook 一键安装 / 卸载 / 状态检查工具
# 支持目标：
#   1. Google Antigravity   (~/.gemini/config/hooks.json)
#   2. Anthropic Claude Code (~/.claude/settings.json)
#   3. pi                    (~/.pi/agent/extensions/kapybara-bridge.ts)
#
# 安装模型（v5.4 起）：hook 脚本与 pi 桥被【拷贝】到稳定目录 ~/.kapybara-buddy/，
# 三处全局配置只引用该拷贝。项目目录此后搬家 / 改名 / 删除都不影响已装的 hook；
# 仓库内脚本更新后重跑 install 即同步（status 会提示版本漂移）。
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

KB_HOME="$HOME/.kapybara-buddy"
KB_HOOKS="$KB_HOME/hooks"
KB_PI_EXT="$KB_HOME/pi-extension"

CLAUDE_HOOK_DST="$KB_HOOKS/claude-hook.sh"
AG_HOOK_DST="$KB_HOOKS/antigravity-hook.sh"
PI_BRIDGE_SRC="$REPO_DIR/pi-extension/kapybara-bridge.ts"
PI_BRIDGE_DST="$KB_PI_EXT/kapybara-bridge.ts"
PI_EXT_DIR="$HOME/.pi/agent/extensions"

GEMINI_HOOKS="$HOME/.gemini/config/hooks.json"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"

DATE_TAG=$(date +"%Y%m%d")

ACTION="${1:-status}"

# 旧版安装把仓库绝对路径烤进全局配置，搬家即失效；install/uninstall 都会清理
LEGACY_MARKERS=("agents/claude/hook.sh" "agents/antigravity/hook.sh")
NEW_MARKER=".kapybara-buddy/hooks/"

do_status() {
  echo "=== Kapybara Buddy Hook 状态检查 ==="

  # 桌宠进程
  if pgrep -f "kapybara-buddy-target/release/kapybara-buddy" >/dev/null 2>&1; then
    echo "• 桌宠进程: 运行中"
  else
    echo "• 桌宠进程: 未运行（./start.sh 启动；hook 推事件时静默跳过）"
  fi

  # 安装目录（拷贝模式的核心）
  if [ -x "$CLAUDE_HOOK_DST" ] && [ -x "$AG_HOOK_DST" ]; then
    echo "• 稳定安装目录 ~/.kapybara-buddy/: 就绪"
  else
    echo "• 稳定安装目录 ~/.kapybara-buddy/: 缺失（请重跑 install）"
  fi

  # 1. Antigravity
  if [ -f "$GEMINI_HOOKS" ]; then
    AG_RES=$(node -e '
      const fs = require("fs"), cp = require("child_process");
      try {
        const d = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        const sec = d["kapybara-buddy"];
        if (!sec) { console.log("未安装"); process.exit(0); }
        const cmds = JSON.stringify(sec).match(/"command"\s*:\s*"([^"]*antigravity-hook\.sh[^"]*)"/) || [];
        const cmd = cmds[1] || "";
        const path = cmd.split(" ")[0];
        const exists = path && fs.existsSync(path);
        const valid = exists ? "路径有效" : "已失效(脚本不存在)，请重装";
        console.log("已安装 · " + valid);
      } catch(e) { console.log("配置解析失败"); }
    ' "$GEMINI_HOOKS" 2>/dev/null || echo "node 不可用，无法解析")
    echo "• Antigravity (~/.gemini/config/hooks.json): $AG_RES"
  else
    echo "• Antigravity (~/.gemini/config/hooks.json): 配置文件不存在"
  fi

  # 2. Claude Code
  if [ -f "$CLAUDE_SETTINGS" ]; then
    CL_RES=$(node -e '
      const fs = require("fs");
      try {
        const d = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        const str = JSON.stringify(d.hooks || {});
        const m = str.match(/[^"]*kapybara-buddy\/hooks\/claude-hook\.sh/);
        if (!m) { console.log(str.includes("agents/claude/hook.sh") ? "旧版安装(指向项目路径，已随搬家失效)，请重装" : "未安装"); process.exit(0); }
        const path = m[0].replace(/\\\//g, "/");
        console.log(fs.existsSync(path) ? "已安装 · 路径有效" : "已失效(脚本不存在)，请重装");
      } catch(e) { console.log("配置解析失败"); }
    ' "$CLAUDE_SETTINGS" 2>/dev/null || echo "node 不可用，无法解析")
    echo "• Claude Code (~/.claude/settings.json): $CL_RES"
  else
    echo "• Claude Code (~/.claude/settings.json): 配置文件不存在"
  fi

  # 3. pi 扩展
  if [ -L "$PI_EXT_DIR/kapybara-bridge.ts" ]; then
    echo "• pi 扩展 (~/.pi/agent/extensions/kapybara-bridge.ts): 软链模式（搬家会断，建议重跑 install 改为拷贝）"
  elif [ -f "$PI_EXT_DIR/kapybara-bridge.ts" ]; then
    if [ -f "$PI_BRIDGE_DST" ] && cmp -s "$PI_EXT_DIR/kapybara-bridge.ts" "$PI_BRIDGE_DST"; then
      echo "• pi 扩展 (~/.pi/agent/extensions/kapybara-bridge.ts): 已安装 · 拷贝模式"
    else
      echo "• pi 扩展 (~/.pi/agent/extensions/kapybara-bridge.ts): 已安装 · 与 ~/.kapybara-buddy 副本不一致，请重跑 install"
    fi
  else
    echo "• pi 扩展 (~/.pi/agent/extensions/kapybara-bridge.ts): 未安装"
  fi

  # 4. 仓库脚本 vs 已装拷贝的版本漂移
  if [ -f "$KB_HOOKS/claude-hook.sh" ]; then
    if cmp -s "$REPO_DIR/agents/claude/hook.sh" "$CLAUDE_HOOK_DST" && \
       cmp -s "$REPO_DIR/agents/antigravity/hook.sh" "$AG_HOOK_DST" && \
       { [ ! -f "$PI_BRIDGE_SRC" ] || cmp -s "$PI_BRIDGE_SRC" "$PI_BRIDGE_DST"; }; then
      echo "• 版本: 与仓库一致"
    else
      echo "• 版本: 仓库脚本有更新，重跑 install 同步"
    fi
  fi
}

do_install() {
  echo "=== 开始安装 Kapybara Buddy Hooks ==="

  # 0. 拷贝脚本到稳定目录（全局配置只指向这里，项目搬家不影响）
  mkdir -p "$KB_HOOKS" "$KB_PI_EXT"
  cp "$REPO_DIR/agents/claude/hook.sh"       "$CLAUDE_HOOK_DST"
  cp "$REPO_DIR/agents/antigravity/hook.sh"  "$AG_HOOK_DST"
  chmod +x "$CLAUDE_HOOK_DST" "$AG_HOOK_DST"
  if [ -f "$PI_BRIDGE_SRC" ]; then
    cp "$PI_BRIDGE_SRC" "$PI_BRIDGE_DST"
  else
    echo "⚠ 未找到 $PI_BRIDGE_SRC，跳过 pi 桥拷贝"
  fi
  echo "✔ hook 脚本与 pi 桥已拷贝到 ~/.kapybara-buddy/"

  # 1. Antigravity 注入
  mkdir -p "$(dirname "$GEMINI_HOOKS")"
  if [ -f "$GEMINI_HOOKS" ]; then
    BAK_FILE="$GEMINI_HOOKS.bak-$DATE_TAG"
    echo "• 备份 Antigravity 配置 -> $BAK_FILE"
    cp -p "$GEMINI_HOOKS" "$BAK_FILE"
  fi

  node -e '
    const fs = require("fs");
    const p = process.argv[1];
    const hook = process.argv[2];
    let data = {};
    if (fs.existsSync(p)) {
      try { data = JSON.parse(fs.readFileSync(p, "utf8")); } catch(e) { data = {}; }
    }
    // 清理旧版指向项目路径的条目
    for (const ev of Object.keys(data)) {
      if (ev === "kapybara-buddy") continue;
      const s = JSON.stringify(data[ev]);
      if (s.includes("agents/antigravity/hook.sh")) delete data[ev];
    }
    data["kapybara-buddy"] = {
      "PreInvocation": [
        {
          "type": "command",
          "command": `${hook} PreInvocation`,
          "timeout": 5
        }
      ],
      "PreToolUse": [
        {
          "matcher": "*",
          "hooks": [
            {
              "type": "command",
              "command": `${hook} PreToolUse`,
              "timeout": 5
            }
          ]
        }
      ],
      "PostToolUse": [
        {
          "matcher": "*",
          "hooks": [
            {
              "type": "command",
              "command": `${hook} PostToolUse`,
              "timeout": 5
            }
          ]
        }
      ],
      "Stop": [
        {
          "type": "command",
          "command": `${hook} Stop`,
          "timeout": 5
        }
      ]
    };
    fs.writeFileSync(p, JSON.stringify(data, null, 2), "utf8");
    console.log("✔ Antigravity hooks 已注入:", p);
  ' "$GEMINI_HOOKS" "$AG_HOOK_DST"

  # 2. Claude Code 注入
  mkdir -p "$(dirname "$CLAUDE_SETTINGS")"
  if [ -f "$CLAUDE_SETTINGS" ]; then
    BAK_FILE="$CLAUDE_SETTINGS.bak-$DATE_TAG"
    echo "• 备份 Claude Code 配置 -> $BAK_FILE"
    cp -p "$CLAUDE_SETTINGS" "$BAK_FILE"
  fi

  node -e '
    const fs = require("fs");
    const p = process.argv[1];
    const hook = process.argv[2];
    let data = {};
    if (fs.existsSync(p)) {
      try { data = JSON.parse(fs.readFileSync(p, "utf8")); } catch(e) { data = {}; }
    }
    if (!data.hooks) data.hooks = {};

    // 先清掉旧版指向项目路径的条目（搬家已失效的死配置）
    for (const k of Object.keys(data.hooks)) {
      if (Array.isArray(data.hooks[k])) {
        data.hooks[k] = data.hooks[k].filter(entry => {
          return !((entry.hooks || []).some(h => (h.command || "").includes("agents/claude/hook.sh")));
        });
      }
    }

    const events = [
      { name: "SessionStart", matcher: "", cmd: `${hook} SessionStart` },
      { name: "UserPromptSubmit", matcher: "", cmd: `${hook} UserPromptSubmit` },
      { name: "PreToolUse", matcher: "*", cmd: `${hook} PreToolUse` },
      { name: "PostToolUse", matcher: "*", cmd: `${hook} PostToolUse` },
      { name: "PostToolUseFailure", matcher: "*", cmd: `${hook} PostToolUseFailure` },
      { name: "Stop", matcher: "", cmd: `${hook} Stop` },
      { name: "PreCompact", matcher: "", cmd: `${hook} PreCompact` },
      { name: "SessionEnd", matcher: "", cmd: `${hook} SessionEnd` }
    ];

    for (const ev of events) {
      if (!Array.isArray(data.hooks[ev.name])) data.hooks[ev.name] = [];
      // 检查是否已包含
      const exists = data.hooks[ev.name].some(entry => {
        return (entry.hooks || []).some(h => (h.command || "").includes(".kapybara-buddy/hooks/claude-hook.sh"));
      });
      if (!exists) {
        const item = {
          matcher: ev.matcher,
          hooks: [
            {
              type: "command",
              command: ev.cmd,
              timeout: 5
            }
          ]
        };
        data.hooks[ev.name].push(item);
      }
    }

    fs.writeFileSync(p, JSON.stringify(data, null, 2), "utf8");
    console.log("✔ Claude Code hooks 已注入:", p);
  ' "$CLAUDE_SETTINGS" "$CLAUDE_HOOK_DST"

  # 3. pi 扩展（拷贝模式，替代旧软链；软链随项目搬家断链）
  if [ -f "$PI_BRIDGE_SRC" ]; then
    mkdir -p "$PI_EXT_DIR"
    rm -f "$PI_EXT_DIR/kapybara-bridge.ts"
    cp "$PI_BRIDGE_DST" "$PI_EXT_DIR/kapybara-bridge.ts"
    echo "✔ pi 扩展已安装: $PI_EXT_DIR/kapybara-bridge.ts（拷贝模式）"
  fi

  echo "=== 安装完成！hook 指向 ~/.kapybara-buddy/ 拷贝，项目搬家不再影响 ==="
  echo "=== 请确保 kapybara-buddy (./start.sh) 处于运行状态 ==="
}

do_uninstall() {
  echo "=== 开始卸载 Kapybara Buddy Hooks ==="

  # 1. Antigravity 卸载
  if [ -f "$GEMINI_HOOKS" ]; then
    node -e '
      const fs = require("fs");
      const p = process.argv[1];
      try {
        const data = JSON.parse(fs.readFileSync(p, "utf8"));
        let removed = false;
        if (data["kapybara-buddy"]) { delete data["kapybara-buddy"]; removed = true; }
        // 清理旧版指向项目路径的条目
        for (const ev of Object.keys(data)) {
          if (JSON.stringify(data[ev]).includes("agents/antigravity/hook.sh")) { delete data[ev]; removed = true; }
        }
        if (removed) {
          fs.writeFileSync(p, JSON.stringify(data, null, 2), "utf8");
          console.log("✔ 已从 Antigravity 移除 kapybara-buddy 配置");
        }
      } catch(e) {}
    ' "$GEMINI_HOOKS"
  fi

  # 2. Claude Code 卸载
  if [ -f "$CLAUDE_SETTINGS" ]; then
    node -e '
      const fs = require("fs");
      const p = process.argv[1];
      try {
        const data = JSON.parse(fs.readFileSync(p, "utf8"));
        if (data.hooks) {
          for (const k of Object.keys(data.hooks)) {
            if (Array.isArray(data.hooks[k])) {
              data.hooks[k] = data.hooks[k].filter(entry => {
                const cmds = (entry.hooks || []).map(h => h.command || "").join(" ");
                return !cmds.includes(".kapybara-buddy/hooks/claude-hook.sh") && !cmds.includes("agents/claude/hook.sh");
              });
            }
          }
          fs.writeFileSync(p, JSON.stringify(data, null, 2), "utf8");
          console.log("✔ 已从 Claude Code 移除 kapybara-buddy 配置");
        }
      } catch(e) {}
    ' "$CLAUDE_SETTINGS"
  fi

  # 3. pi 扩展卸载
  if [ -e "$PI_EXT_DIR/kapybara-bridge.ts" ] || [ -L "$PI_EXT_DIR/kapybara-bridge.ts" ]; then
    rm -f "$PI_EXT_DIR/kapybara-bridge.ts"
    echo "✔ 已移除 pi 扩展: $PI_EXT_DIR/kapybara-bridge.ts"
  fi

  # 4. 稳定目录
  if [ -d "$KB_HOME" ]; then
    rm -rf "$KB_HOME"
    echo "✔ 已删除稳定安装目录 ~/.kapybara-buddy/"
  fi

  echo "=== 卸载完成 ==="
}

case "$ACTION" in
  install)
    do_install
    ;;
  uninstall)
    do_uninstall
    ;;
  status|*)
    do_status
    ;;
esac
