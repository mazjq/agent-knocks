@echo off
rem One-click uninstaller for the Rust build: double-click this file.
setlocal
echo Uninstalling Agent Knocks (Rust build)...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1" %*
echo.
echo Done. You can close this window.
pause
