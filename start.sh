#!/bin/bash
# kapybara-buddy 启停（macOS）
cd "$(dirname "$0")" || exit 1
case "${1:-start}" in
  start)
    if pgrep -f "Electron .*kapybara-buddy|electron \." >/dev/null 2>&1; then
      echo "kapybara-buddy 已在运行"
    else
      nohup ./node_modules/.bin/electron . >> "$HOME/.kapybara-buddy.log" 2>&1 &
      disown
      echo "kapybara-buddy 已启动 (pid $!) · 按 Option+G 呼出"
    fi
    ;;
  stop)
    pkill -f "kapybara-buddy/node_modules/.bin|electron \." 2>/dev/null && echo "已停止" || echo "未在运行"
    pkill -f "2026-09_kapybara-buddy.*Electron" 2>/dev/null; true
    ;;
  log)
    tail -30 "$HOME/.kapybara-buddy.log"
    ;;
  *)
    echo "用法: ./start.sh [start|stop|log]"
    ;;
esac
