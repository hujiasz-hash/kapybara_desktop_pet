#!/usr/bin/env bash
set -e

# ==============================================================================
# kapybara-buddy Agent Hook 一键安装 / 卸载 / 状态检查工具
# 支持目标：
#   1. Google Antigravity (~/.gemini/config/hooks.json)
#   2. Anthropic Claude Code (~/.claude/settings.json)
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

GEMINI_HOOKS="$HOME/.gemini/config/hooks.json"
CLAUDE_SETTINGS="$HOME/.claude/settings.json"

DATE_TAG=$(date +"%Y%m%d")

ACTION="${1:-status}"

do_status() {
  echo "=== Kapybara Buddy Hook 状态检查 ==="
  
  # 1. Antigravity
  if [ -f "$GEMINI_HOOKS" ]; then
    HAS_AG=$(node -e '
      try {
        const d = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
        console.log(d["kapybara-buddy"] ? "已安装" : "未安装");
      } catch(e) { console.log("配置解析失败"); }
    ' "$GEMINI_HOOKS")
    echo "• Antigravity (~/.gemini/config/hooks.json): $HAS_AG"
  else
    echo "• Antigravity (~/.gemini/config/hooks.json): 配置文件不存在"
  fi

  # 2. Claude Code
  if [ -f "$CLAUDE_SETTINGS" ]; then
    HAS_CLAUDE=$(node -e '
      try {
        const d = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
        const str = JSON.stringify(d.hooks || {});
        console.log(str.includes("kapybara-buddy") ? "已安装" : "未安装");
      } catch(e) { console.log("配置解析失败"); }
    ' "$CLAUDE_SETTINGS")
    echo "• Claude Code (~/.claude/settings.json): $HAS_CLAUDE"
  else
    echo "• Claude Code (~/.claude/settings.json): 配置文件不存在"
  fi
}

do_install() {
  echo "=== 开始安装 Kapybara Buddy Hooks ==="

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
    const repo = process.argv[2];
    let data = {};
    if (fs.existsSync(p)) {
      try { data = JSON.parse(fs.readFileSync(p, "utf8")); } catch(e) { data = {}; }
    }
    data["kapybara-buddy"] = {
      "PreInvocation": [
        {
          "type": "command",
          "command": `${repo}/agents/antigravity/hook.sh PreInvocation`,
          "timeout": 5
        }
      ],
      "PreToolUse": [
        {
          "matcher": "*",
          "hooks": [
            {
              "type": "command",
              "command": `${repo}/agents/antigravity/hook.sh PreToolUse`,
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
              "command": `${repo}/agents/antigravity/hook.sh PostToolUse`,
              "timeout": 5
            }
          ]
        }
      ],
      "Stop": [
        {
          "type": "command",
          "command": `${repo}/agents/antigravity/hook.sh Stop`,
          "timeout": 5
        }
      ]
    };
    fs.writeFileSync(p, JSON.stringify(data, null, 2), "utf8");
    console.log("✔ Antigravity hooks 已注入:", p);
  ' "$GEMINI_HOOKS" "$REPO_DIR"

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
    const repo = process.argv[2];
    let data = {};
    if (fs.existsSync(p)) {
      try { data = JSON.parse(fs.readFileSync(p, "utf8")); } catch(e) { data = {}; }
    }
    if (!data.hooks) data.hooks = {};

    const events = [
      { name: "SessionStart", matcher: "", cmd: `${repo}/agents/claude/hook.sh SessionStart` },
      { name: "UserPromptSubmit", matcher: "", cmd: `${repo}/agents/claude/hook.sh UserPromptSubmit` },
      { name: "PreToolUse", matcher: "*", cmd: `${repo}/agents/claude/hook.sh PreToolUse` },
      { name: "PostToolUse", matcher: "*", cmd: `${repo}/agents/claude/hook.sh PostToolUse` },
      { name: "PostToolUseFailure", matcher: "*", cmd: `${repo}/agents/claude/hook.sh PostToolUseFailure` },
      { name: "Stop", matcher: "", cmd: `${repo}/agents/claude/hook.sh Stop` },
      { name: "PreCompact", matcher: "", cmd: `${repo}/agents/claude/hook.sh PreCompact` },
      { name: "SessionEnd", matcher: "", cmd: `${repo}/agents/claude/hook.sh SessionEnd` }
    ];

    for (const ev of events) {
      if (!Array.isArray(data.hooks[ev.name])) data.hooks[ev.name] = [];
      // 检查是否已包含
      const exists = data.hooks[ev.name].some(entry => {
        return (entry.hooks || []).some(h => (h.command || "").includes("agents/claude/hook.sh"));
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
  ' "$CLAUDE_SETTINGS" "$REPO_DIR"

  echo "=== 安装完成！请确保 kapybara-buddy (./start.sh) 处于运行状态 ==="
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
        if (data["kapybara-buddy"]) {
          delete data["kapybara-buddy"];
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
                return !(entry.hooks || []).some(h => (h.command || "").includes("agents/claude/hook.sh"));
              });
            }
          }
          fs.writeFileSync(p, JSON.stringify(data, null, 2), "utf8");
          console.log("✔ 已从 Claude Code 移除 kapybara-buddy 配置");
        }
      } catch(e) {}
    ' "$CLAUDE_SETTINGS"
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
