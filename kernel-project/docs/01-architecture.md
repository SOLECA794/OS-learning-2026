# 01 · 总体架构设计

## 1. 内核形态

**Linux 兼容宏内核**（monolithic kernel with Linux UAPI compatibility）：

- 内核与用户态位于同一地址空间边界（内核高地址 / 用户低地址），系统调用直接陷入。
- 无微内核式的 IPC 转发层——这是 CAgent（syscall 兼容测例）与 BuildStorm（性能）两项评分的架构基础。
- 用户态语义对齐 Linux UAPI 6.x（结构体布局、errno、flag 语义），内部对象模型（进程、文件、地址空间）为自研设计，不复制 Linux 内部结构。

## 2. 分层结构

```
┌─────────────────────────────────────────────┐
│                用户态（测试程序/LTP/工具链）      │
├─────────────────────────────────────────────┤
│  syscall 层      src/kernel/syscall/         │  ABI 边界：参数校验、errno 映射、用户指针安全
├─────────────────────────────────────────────┤
│  核心子系统      src/kernel/                 │  mm / task / sched / signal / ipc / event / time
├─────────────────────────────────────────────┤
│  文件系统        src/kernel/fs/              │  VFS 抽象 + 具体 fs（当前 ramfs/staticfs）
├─────────────────────────────────────────────┤
│  网络            src/net/                    │  socket 抽象 + 协议实现（自研，见 06）
├─────────────────────────────────────────────┤
│  驱动            src/driver/                 │  设备框架（FDT 驱动）+ virtio 等具体驱动
├─────────────────────────────────────────────┤
│  arch 抽象       src/arch/                   │  唯一架构差异入口（riscv64 / loongarch64）
├─────────────────────────────────────────────┤
│  klib            src/klib/                   │  分配器接口、锁原语、日志、容器工具
└─────────────────────────────────────────────┘
```

**依赖规则**：

- 上层只允许依赖下层；`src/arch/` 是唯一允许出现架构分支的层。
- 子系统之间禁止互相摸内部状态：跨子系统交互必须走显式接口（窄接口原则）。
- 用户态数据（指针、结构体）只在 syscall 边界解析为内部类型，进入核心子系统后不得出现 Linux ABI 布局。

## 3. 双架构策略

- `src/arch/mod.rs` 通过条件编译选择 `riscv64` 或 `loongarch64` 后端，对外暴露统一 API（`UserContext`、`KernelContext`、`SigContext`、`PageTable` 等类型别名）。
- 所有内核代码只引用 `crate::arch::*`，禁止直接触碰架构私有路径。
- 新功能落地规则：共享逻辑 + 双架构实现 + 双架构验证一次完成（不允许"先 RV 后 LA"造成技术债）。

## 4. 启动流程（Boot Sequence）

```
arch 早期启动（汇编 + 早期映射）
  → kernel::main(hartid, heap_start, memory_top)
      1. kalloc::init         内核堆
      2. mm::init             物理页分配器
      3. driver::init / arch::init / fs::init   设备树探测 + 驱动匹配
      4. fs::mount_init_fs    挂载根文件系统
      5. task::create_initprocess   创建首个用户进程
      6. timer::init          定时器
      7. arch::setup_all_cores → 使能中断 → 进入调度器
```

## 5. 模块职责（子目录）

| 目录 | 职责 | 关键接口 |
|---|---|---|
| `arch/` | 架构抽象、上下文切换、页表、异常入口 | `arch_export!` 宏统一导出 |
| `kernel/mm/` | 物理页分配、AddrSpace、CoW、mmap、缺页、swap（可选） | `AddrSpace`、`MapArea` |
| `kernel/task/` + `scheduler/` | PCB/TCB 分层、调度器、内核线程、处理器 | `Task` trait、`Processor` |
| `kernel/syscall/` | 分发与 ABI 校验 | `syscall(num, args)` → `SysResult<usize>` |
| `kernel/event/` | 事件驱动唤醒：WaitQueue、Poll、Timer | `Event` 位掩码 |
| `kernel/ipc/` | 管道、POSIX 信号、共享内存 | — |
| `kernel/usync/` | futex 等用户同步原语 | — |
| `kernel/fs/` | VFS 节点接口 + ramfs/staticfs；页缓存尚未实现 | `VfsNode`、`OpenFileDesc` |
| `driver/` | FDT 设备框架 + 具体驱动 | `Device`/`Driver`、compatible 匹配 |
| `net/` | socket + TCP/UDP + virtio-net | — |
| `klib/` | 分配器、SpinLock、InitedCell、日志宏 | — |

## 6. 关键设计决策（ADR 摘要）

| 决策 | 选择 | 理由 | 状态 |
|---|---|---|---|
| 内核形态 | 宏内核 | 赛制（syscall 兼容 + 性能）奖励宏内核；6/6 一等奖实证 | 已定 |
| 任务模型 | PCB/TCB 分层（Linux 风格） | 兼容测例语义对齐成本最低 | 已定 |
| 调度器 | 多核就绪队列（先全局 FIFO，后按需优化） | 起步简单，性能由 profile 驱动演进 | 起点 |
| 文件系统 | 自研 VFS + 先 tmpfs/ramfs，后 ext4 | 兼容面与工作量平衡 | 起点 |
| 网络 | 自研协议栈（socket → TCP/UDP） | 差异化成亮点；暂借 smoltcp 必须带退出条件 | 已定路线 |
| 构建 | Kconfig → build.mk → Cargo feature | 配置可审计、CI 可矩阵化 | 已定 |

## 7. 允许引入的第三方依赖（白名单方向，逐项评审）

- 分配器：`buddy_system_allocator`（或自研 buddy-slab，二选一，自研优先）
- 数据结构：`hashbrown`、`rangemap` 等无锁/通用容器
- 体系结构寄存器：`riscv`、`loongarch64`（寄存器定义）、`sbi-rt`
- 设备树解析：`fdt` / `device_tree`
- 驱动寄存器：`virtio-drivers`（仅作为映射与队列原语，I/O 路径自研）

> 禁止引入：完整内核（ArceOS/Starry/rCore 代码）、完整协议栈（smoltcp 仅限过渡）、教学文件系统（easyfs 等）。

## 8. 架构演进约束

- 任何模块重构必须先更新本文档对应章节，再动代码（文档与代码同步）。
- 子系统间接口冻结期：M1 骨架完成后，核心接口变更需团队评审（RFC），避免多组并行互相踩。
