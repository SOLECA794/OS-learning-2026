# ROADMAP

> 本文件维护当前阶段与下一阶段目标，一页看清项目状态。
> 周期：一年，终极目标国赛。所有模块按工业级标准做透，不做短期妥协。

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

### 开发策略

- 每个模块按工业级自洽标准做透，不留技术债。
- Spike 仅用于快速定位真实 Linux 程序依赖的 syscall 集合；每个 syscall 仍按 Linux 语义补全校验、errno、边界断言和有界测试。
- 架构同步关卡：RV 每固化一个核心里程碑，立即同步推 LA64 到同一水平线，不让两端出现代差级分叉。

### 下一阶段

#### P0 动态内存与 IPC 基础（工业级 VMA 模型）

- [ ] **brk + mmap/munmap**：VMA 红黑树/B 树管理，区分匿名映射与文件映射，支持 demand paging，munmap 范围拆分。不是 bump 指针。
- [ ] **pipe/pipe2 + dup2/dup3**：shell 管道与测试框架 stdout 捕获。

#### P1 VFS 与存储架构升级

- [ ] **VirtIO-Block 驱动 + 块设备抽象层**：结束 include_bytes! 内嵌 ELF。
- [ ] **Page Cache 与脏页回写**：所有文件操作基于块和缓存，不是同步内存树。
- [ ] 真正 cwd（chdir/getcwd）、unlinkat/rename。
- [ ] **信号全链路**：POSIX 信号、sigmask、sigaltstack、sigreturn 现场恢复，不只 kill。

#### P2 架构同步与多核契约

- [ ] **LA64 用户态对齐**：跟随 RV P0/P1 里程碑同步推进，不长期分叉。
- [ ] **SMP 数据结构预设计**：Per-CPU 调度队列、细粒度自旋锁接口预留；先跑单核负载，接口具备多核并发契约。从核唤醒、IPI 在国赛阶段落地。

#### P3 阶段标尺（每阶段验收标志）

| 阶段 | 验收标准 |
|---|---|
| A | 原生跑通完整静态编译 BusyBox 套件（shell 脚本 + 管道交互） |
| B | Lua / QuickJS / Pthread 基础测例运行 |
| C | LTP 核心子集高通过率 |
| 终极 | 内核内自举编译 GCC/Make |

### 边界

- 当前单核（从核 park）；SMP 数据结构预留，不实际唤醒从核。
- 测试入口均为有界 QEMU 运行，每攻克一个复杂特性保留量化回归证据。
