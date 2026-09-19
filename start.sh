#!/bin/bash
# gemini-buddy 启停（macOS）
cd "$(dirname "$0")" || exit 1
case "${1:-start}" in
  start)
    if pgrep -f "Electron .*gemini-buddy|electron \." >/dev/null 2>&1; then
      echo "gemini-buddy 已在运行"
    else
      nohup ./node_modules/.bin/electron . >> "$HOME/.gemini-buddy.log" 2>&1 &
      disown
      echo "gemini-buddy 已启动 (pid $!) · 按 Option+G 呼出"
    fi
    ;;
  stop)
    pkill -f "gemini-buddy/node_modules/.bin|electron \." 2>/dev/null && echo "已停止" || echo "未在运行"
    pkill -f "2026-09_gemini-buddy.*Electron" 2>/dev/null; true
    ;;
  log)
    tail -30 "$HOME/.gemini-buddy.log"
    ;;
  *)
    echo "用法: ./start.sh [start|stop|log]"
    ;;
esac
