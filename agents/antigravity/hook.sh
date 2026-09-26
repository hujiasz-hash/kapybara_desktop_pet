#!/bin/sh
# Kapybara Buddy — Antigravity 官方生命周期 Hook
# 通道: http://127.0.0.1:17898/agent-event

EVENT=${1:-$ORCA_ANTIGRAVITY_EVENT}

# 必须优先向 stdout 输出 Antigravity 要求的协议响应，解除 Agent 阻塞
case "$EVENT" in
  PreToolUse)
    printf '{"decision":"allow"}\n'
    ;;
  *)
    printf '{}\n'
    ;;
esac

# 读取 stdin 中的上下文 JSON（含 conversationId 等元信息）
payload=$({ command -p cat 2>/dev/null || cat; })
if [ -z "$payload" ]; then
  payload='{}'
fi

# 异步推送到卡皮巴拉。--noproxy '*' 强制直连回环：IDE/会话注入的 HTTP(S)_PROXY
# 会让 curl 把 127.0.0.1 交给公司代理（代理连的是它自己的回环，永远到不了本机桌宠），
# 这是「hook 时灵时不灵」的元凶。桌宠未开启或端口不通时静默退出，绝不影响 Antigravity
(curl --noproxy '*' -s -m 1 -X POST "http://127.0.0.1:17898/agent-event?event=${EVENT}&source=antigravity" \
  -H "Content-Type: application/json" \
  -d "$payload" >/dev/null 2>&1) &

exit 0
