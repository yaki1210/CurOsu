@echo off
rem Curosu installer: elevates to admin, then runs install-uiaccess.ps1
rem (installs curosu.exe to Program Files and signs it with a local
rem  self-signed certificate so UIAccess can take effect).
setlocal

net session >nul 2>&1
if %errorlevel% neq 0 (
    echo Requesting administrator privileges...
    powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
    exit /b
)

powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-uiaccess.ps1" -ExePath "%~dp0curosu.exe"
pause
