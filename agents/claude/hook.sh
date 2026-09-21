#!/bin/sh
# Kapybara Buddy — Claude Code 官方生命周期 Hook
# 通道: http://127.0.0.1:17898/agent-event

EVENT=$1

# Claude Code 要求 Hook 命令向 stdout 输出有效 JSON
printf '{}\n'

# 读取 stdin 中的上下文 JSON（含 session_id 等元信息）
payload=$({ command -p cat 2>/dev/null || cat; })
if [ -z "$payload" ]; then
  payload='{}'
fi

# 异步推送到卡皮巴拉（桌宠未开启或端口不通时静默退出，超时 200ms，绝不影响 Claude Code）
(curl -s -m 0.2 -X POST "http://127.0.0.1:17898/agent-event?event=${EVENT}&source=claude" \
  -H "Content-Type: application/json" \
  -d "$payload" >/dev/null 2>&1) &

exit 0
