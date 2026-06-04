@echo off
rem One-click installer for the Rust build: double-click this file.
setlocal
echo Installing Agent Knocks (Rust build)...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
echo.
echo Done. You can close this window.
pause
