#!/bin/bash
# kapybara-buddy 启停（macOS，Tauri 版）
# 产物目录在 ~/Library/Caches（项目在 ~/Desktop 下受 iCloud 同步管理，编译产物放那里会被
# fileproviderd 动元数据导致 AMFI 误杀，见 src-tauri/.cargo/config.toml 注释）
cd "$(dirname "$0")" || exit 1
BIN="$HOME/Library/Caches/kapybara-buddy-target/release/kapybara-buddy"
case "${1:-start}" in
  start)
    if pgrep -f "kapybara-buddy-target/release/kapybara-buddy" >/dev/null 2>&1; then
      echo "kapybara-buddy 已在运行"
    else
      if [ ! -x "$BIN" ]; then
        echo "首次运行：编译 release（需要 Rust 工具链和 Homebrew llvm/lld，见 README）..."
        (cd src-tauri && cargo build --release) || exit 1
      fi
      nohup "$BIN" >> "$HOME/.kapybara-buddy.log" 2>&1 &
      disown
      echo "kapybara-buddy 已启动 (pid $!) · 按 Option+G 呼出"
    fi
    ;;
  stop)
    pkill -f "kapybara-buddy-target/release/kapybara-buddy" 2>/dev/null && echo "已停止" || echo "未在运行"
    ;;
  log)
    tail -30 "$HOME/.kapybara-buddy.log"
    ;;
  build)
    (cd src-tauri && cargo build --release)
    ;;
  *)
    echo "用法: ./start.sh [start|stop|log|build]"
    ;;
esac
