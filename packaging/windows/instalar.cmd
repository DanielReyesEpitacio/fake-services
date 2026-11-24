@echo off
rem Instala Fake Services para tu usuario (doble clic).
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0instalar.ps1" %*
pause
