@echo off
chcp 65001 >nul
rem 双击即可：安装 HoverNote、建开始菜单快捷方式、开机自启，然后启动。
rem 脚本在 tools\ 下，这里给的必须是全路径——%~dp0 已经带了结尾的反斜杠。
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0tools\install.ps1" %*
echo.
pause
