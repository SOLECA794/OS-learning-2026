# 2026-10-15 WSL 跨盘 git 操作卡顿

## 现象
- 在 WSL Ubuntu-24.04 内对 `/mnt/d/Desktop/OS-learning` 执行 `git switch` / `git status`，单次操作卡 5–6 分钟无响应。
- 期间 CPU 0%，磁盘 IO 等待。

## 根因
- WSL2 跨 9P 文件系统访问 Windows D 盘，metadata 缓存与 git 大量小文件 stat 叠加，性能急剧下降。

## 处置
- 杀掉卡住的 git 进程重试；或改用 Windows 侧 PowerShell 直接执行 git（`Set-Location D:\Desktop\OS-learning; git ...`），Windows 原生 IO 无此问题。

## 预防
- git 操作优先用 Windows 侧 PowerShell。
- WSL 内只跑构建/测试（cargo/qemu），不做 git 元数据操作。
