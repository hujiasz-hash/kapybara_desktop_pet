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

# 异步推送到卡皮巴拉。--noproxy '*' 强制直连回环：Claude Code 会话注入的
# HTTP(S)_PROXY 会让 curl 把 127.0.0.1 交给公司代理（代理连的是它自己的回环，
# 永远到不了本机桌宠）——这是「hook 时灵时不灵」的元凶。桌面宠未开启或端口不通
# 时静默退出，绝不影响 Claude Code
(curl --noproxy '*' -s -m 1 -X POST "http://127.0.0.1:17898/agent-event?event=${EVENT}&source=claude" \
  -H "Content-Type: application/json" \
  -d "$payload" >/dev/null 2>&1) &

exit 0
