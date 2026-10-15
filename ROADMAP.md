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

### 下一阶段（按竞赛收益排序）

**策略转变**：从"对着 Linux Manual 逐个 RFC"转向"拿 BusyBox sh 当靶子 Spike，缺什么 syscall 临时 stub，跑通后再补校验和测试固化"。

#### P0 跑真实 Linux 程序的硬前提

- [ ] **brk + 匿名 mmap/munmap**：用户堆扩展。musl/glibc 启动必须通过 brk 或匿名 mmap 初始化 malloc，没有这俩任何真实编译的 Linux 程序都起不来。
- [ ] **pipe/pipe2 + dup2/dup3**：shell 管道（ls | grep）和测试框架捕获 stdout 的核心依赖。

#### P1 脱离玩具状态

- [ ] 真正 cwd（chdir/getcwd，相对路径不再 fallback 到 root）
- [ ] unlinkat / rename（解除"目录游标稳定依赖无删除"假设）
- [ ] 信号（POSIX 基本信号，先做 SIGCHLD 即可）

#### P2 集中冲刺（RV 跑通 BusyBox 后）

- [ ] LA64 用户态对齐：不双架构同步纠缠，RV 成熟后一次性 3–5 天集中迁移 Trap/调度/COW。

#### 区域赛后

- [ ] VirtIO-Block 驱动 + FAT32/简易 ext4（结束 include_bytes! 内嵌 ELF）
- [ ] 网络协议栈：区域赛阶段不碰

### 边界

- 纯内存环境，无磁盘/网络/字符设备驱动。
- 测试入口均为有界 QEMU 运行，不跑无界交互版。
- 当前单核（从核 park），不引入 SMP。
