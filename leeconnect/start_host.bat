@echo off
title LeeConnect Host Server
cd /d "%~dp0"
echo Starting LeeConnect Host Server on 0.0.0.0:44330...
"target\release\host.exe" %*
