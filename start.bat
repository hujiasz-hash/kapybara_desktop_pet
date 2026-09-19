@echo off
rem kapybara-buddy 启动（Windows）
cd /d "%~dp0"
start /b "" ".\node_modules\.bin\electron" .
echo kapybara-buddy 已启动 · 按 Alt+G 呼出
