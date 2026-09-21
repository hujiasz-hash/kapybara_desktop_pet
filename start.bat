@echo off
rem kapybara-buddy 启动（Windows，Tauri 版；需要 Rust 工具链 + WebView2）
cd /d "%~dp0"
if not defined CARGO_TARGET_DIR set CARGO_TARGET_DIR=%LOCALAPPDATA%\kapybara-buddy-target
set BIN=%CARGO_TARGET_DIR%\release\kapybara-buddy.exe
if not exist "%BIN%" (
  echo 首次运行：编译 release（需要 Rust 工具链）...
  pushd src-tauri
  cargo build --release || (popd & exit /b 1)
  popd
)
start "" "%BIN%"
echo kapybara-buddy 已启动 · 按 Alt+G 呼出
