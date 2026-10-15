# 11 · 术语表

> 项目内缩写与术语统一解释。外部读者按本文件对照。

| 术语 | 全称 | 含义 |
|---|---|---|
| COW | Copy-On-Write | 写时复制。fork 后父子共享只读页，首次写时才复制。本项目用软件集合实现（双架构 PTE 无空闲位） |
| VFS | Virtual File System | 虚拟文件系统层。本项目 `src/fs/`，不碰 Task 与用户指针 |
| ABI | Application Binary Interface | 系统调用二进制接口（寄存器传参、errno、status 宽度） |
| PTE | Page Table Entry | 页表项 |
| VPN / PPN | Virtual / Physical Page Number | 虚拟页号 / 物理页号 |
| RV64 | RISC-V 64 位 | 本项目主架构，Sv39 分页 |
| LA64 | LoongArch 64 位 | 本项目第二架构，用户态占位未启用 |
| MRU | Most Recently Used | 最近最常使用，frame 分配器复用策略 |
| KTEST | Kernel Test | 内核内建测试事件协议 |
| run-id | — | 单次 QEMU 运行唯一标识，对应 `artifacts/runs/<run-id>/` |
| M0–M4 | Milestone 0–4 | 模块里程碑编号 |
| G0–G6 | Goal 0–6 | 赛题验收里程碑编号 |
| W1–W4 | Week 1–4 | 周任务编号 |
| RFC | Request For Comments | 设计决策记录，存 `log/rfc/` |
| ADR | Architecture Decision Record | 架构决策记录（本项目以 RFC 形式承载） |
| fd | file descriptor | 文件描述符 |
| ramfs | RAM filesystem | 内存文件系统 |
| canary | — | 栈/状态槽尾部校验值，本项目 wait4 status 后 4 字节 canary 用于校验写回宽度 |
