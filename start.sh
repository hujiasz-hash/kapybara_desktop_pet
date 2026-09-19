#!/bin/bash
# gemini-buddy 启停脚本
cd "$(dirname "$0")" || exit 1

case "${1:-start}" in
  start)
    if pgrep -f "buddy\.py" >/dev/null 2>&1; then
      echo "gemini-buddy 已在运行 (pid $(pgrep -f 'buddy\.py' | head -1))"
    else
      nohup python3 buddy.py >> "$HOME/.gemini-buddy.log" 2>&1 &
      disown
      echo "gemini-buddy 已启动 (pid $!) · 按 Option+G 呼出"
    fi
    ;;
  stop)
    if pgrep -f "buddy\.py" >/dev/null 2>&1; then
      pkill -f "buddy\.py" && echo "已停止"
    else
      echo "未在运行"
    fi
    ;;
  log)
    tail -30 "$HOME/.gemini-buddy.log"
    ;;
  *)
    echo "用法: ./start.sh [start|stop|log]"
    ;;
esac
