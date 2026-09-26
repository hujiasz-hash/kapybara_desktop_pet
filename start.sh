#!/bin/bash
# kapybara-buddy 启停（macOS，Tauri 版）
# 产物目录用 CARGO_TARGET_DIR 环境变量注入（与 start.bat 同款可移植写法，任何用户开箱可编）：
# 项目放在 OneDrive/iCloud 等同步目录时，新鲜 dylib 的元数据被 fileproviderd 动过可能触发
# AMFI "Invalid Page" 误杀，所以产物放到同步目录外的 ~/Library/Caches
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/Library/Caches/kapybara-buddy-target}"
cd "$(dirname "$0")" || exit 1
BIN="$CARGO_TARGET_DIR/release/kapybara-buddy"
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
