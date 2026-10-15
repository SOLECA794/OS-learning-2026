# ROADMAP

> 本文件维护当前阶段与下一阶段目标，一页看清项目状态。
> 维护规则：每个里程碑完成后更新"已完成"区块；新目标进入"下一阶段"。

## 当前阶段：G6 完成后稳定期

2026 华东区域赛内核项目（Rust 宏内核，RV64 + LA64 双架构）。

### 已完成（截至 2026-10-15）

| 里程碑 | 内容 | 状态 |
|---|---|---|
| M0–M1 | 双架构 scaffold、trap 分发、分页激活、内核栈/堆/地址空间 | 完成 |
| M1.5 | 用户态切换（RV 验证，LA 占位） | 完成 |
| M2 | 调度器（时钟中断轮转）、上下文切换 | 完成 |
| M3 | wait4 32位 status ABI、openat/mkdirat dirfd、fstat/lseek/positioned-io | 完成 |
| M25 | VFS 层重构（`src/fs/`）、console input | 完成 |
| M28 | COW OOM 520 点注入、wait+COW 回收 128 轮 | 完成 |
| G0 | QEMU 有界运行器 + KTEST 事件 + result.json | 完成 |
| G1 | 连续页复用（双架构 frame allocator） | 完成 |
| G4 | exec + 最小命名程序源 | 完成 |
| G5 | ramfs + FD + read/write/close/dup + 小 shell | 完成 |
| G6 | COW fork（软件 COW，双架构 PTE 无空闲位） | 完成 |
| 卫生 | warnings 清零、COW 引用计数 debug 防护 | 完成 |

### 下一阶段

- [ ] LA64 用户态对齐（trap 分发、内存集、COW 移植）
- [ ] 真正 cwd（相对路径不再 fallback 到 root）
- [ ] 文件删除 / rename（解除"目录游标稳定依赖无删除"假设）
- [ ] 信号 / 管道（POSIX 基本 IPC）
- [ ] 网络协议栈预留（docs/06-networking.md 已有设计，未实现）

### 边界

- 纯内存环境，无磁盘/网络/字符设备驱动。
- 测试入口均为有界 QEMU 运行，不跑无界交互版。
