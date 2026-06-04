@echo off
rem One-click installer: double-click this file.
rem Runs install.ps1 with ExecutionPolicy bypassed, from this folder.
setlocal
echo Installing Agent Knocks...
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install.ps1" %*
echo.
echo Done. You can close this window.
pause
