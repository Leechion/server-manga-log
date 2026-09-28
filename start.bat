@echo off
chcp 65001 >nul
title 服务器改动 · 漫画志
cd /d "%~dp0"
where node >nul 2>nul
if errorlevel 1 (
  echo.
  echo  [!] 没有找到 Node.js，请先安装：https://nodejs.org/  一路下一步即可
  echo.
  pause
  exit /b 1
)
echo  正在开演……浏览器会自动打开，关闭本窗口即闭幕。
node server.js
pause
