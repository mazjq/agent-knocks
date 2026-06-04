@echo off
rem One-click uninstaller: double-click this file.
setlocal
echo Uninstalling Agent Knocks...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1" %*
echo.
echo Done. You can close this window.
pause
