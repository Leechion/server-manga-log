@echo off
title LeeConnect Client
cd /d "%~dp0"
start "" "target\release\gui.exe" %*
