@echo off
chcp 65001 >nul
cd /d D:\Rust项目\barter-rs-main
git add -A
git commit -m "[fix] h_15m P0/P1/P2 问题修复：主循环启用 + 风控接入 + WAL 完善" --author="factory-droid <138933559 factory-droid[bot] users.noreply.github.com>"
