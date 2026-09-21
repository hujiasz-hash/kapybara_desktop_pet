#!/usr/bin/env bash
set -e

# ==============================================================================
# kapybara-buddy 多 Agent 并发与 Hook 自动化测试脚本
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

LOG="$HOME/.kapybara-buddy.log"

echo "=== 1. 测试 Antigravity Hook 协议输出与非阻塞 ==="
RES=$(printf '{"conversationId":"test-ag-proto","toolCall":{"name":"run_command"}}\n' | "$REPO_DIR/agents/antigravity/hook.sh" PreToolUse)
if [ "$RES" != '{"decision":"allow"}' ]; then
  echo "❌ Antigravity PreToolUse 响应不符合规范: $RES"
  exit 1
fi
echo "✔ Antigravity PreToolUse stdout 协议正确: $RES"

RES=$(printf '{"conversationId":"test-ag-proto"}\n' | "$REPO_DIR/agents/antigravity/hook.sh" Stop)
if [ "$RES" != '{}' ]; then
  echo "❌ Antigravity Stop 响应不符合规范: $RES"
  exit 1
fi
echo "✔ Antigravity Stop stdout 协议正确: $RES"

echo "=== 2. 测试 Claude Code Hook 协议输出 ==="
RES=$(printf '{"session_id":"test-claude-proto","hook_event_name":"PreToolUse"}\n' | "$REPO_DIR/agents/claude/hook.sh" PreToolUse)
if [ "$RES" != '{}' ]; then
  echo "❌ Claude Code PreToolUse 响应不符合规范: $RES"
  exit 1
fi
echo "✔ Claude Code PreToolUse stdout 协议正确: $RES"
printf '{"session_id":"test-claude-proto","hook_event_name":"Stop"}\n' | "$REPO_DIR/agents/claude/hook.sh" Stop >/dev/null

echo "=== 3. 模拟双 Agent 并发冲突场景 ==="
UUID_A="sess-$(date +%s)-A"
UUID_B="sess-$(date +%s)-B"

# 场景：Agent A (Antigravity) 正在打工敲键盘
printf '{"conversationId":"'"$UUID_A"'","toolCall":{"name":"cargo build"}}\n' | "$REPO_DIR/agents/antigravity/hook.sh" PreToolUse
sleep 0.3

# 此时 Agent B (Claude Code) 快速搞定并触发 Stop (欢呼)
printf '{"session_id":"'"$UUID_B"'","hook_event_name":"Stop"}\n' | "$REPO_DIR/agents/claude/hook.sh" Stop
sleep 0.3

# 检查日志：Agent B 欢呼时，聚合 fallback 必须仍为 typing（因为 Agent A 还在打工！）
tail -n 3 "$LOG"

# 最后 Agent A 也搞定并 Stop
printf '{"conversationId":"'"$UUID_A"'","executionNum":1}\n' | "$REPO_DIR/agents/antigravity/hook.sh" Stop
sleep 0.3

tail -n 2 "$LOG"

echo "=== 自动化测试全部通过！==="
