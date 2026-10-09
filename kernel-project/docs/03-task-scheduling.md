# 03 · 任务与调度设计

## 1. 目标

提供 Linux 语义的进程/线程模型（fork/exec/exit、wait、信号、多核调度），支撑 CAgent 进程族测例与 BuildStorm 的并发编译负载。

## 2. 组成

```
kernel/task/          PCB/TCB 分层、内核线程、信号、处理器
kernel/scheduler/     调度器（多核就绪队列）
kernel/trap/          异常/中断入口（arch 层装配）
kernel/event/         事件驱动唤醒：Event 位掩码 + WaitQueue + Poll + Timer
kernel/signal/        POSIX 信号（sigaction/siginfo/挂起队列）
kernel/ipc/           管道、共享内存
kernel/usync/         futex
```

## 3. 核心抽象

- **TCB（线程控制块）**：每线程内核栈、用户/内核上下文、信号状态、时间计数、FD 表引用。
- **PCB（进程控制块）**：每进程地址空间、信号动作、工作目录、子/父链、进程组与会话。
- **Task trait**：统一调度接口，由用户任务与内核线程实现。
- **Processor（每 CPU）**：当前运行任务指针、就绪队列、中断使能状态。
- **Scheduler**：起点为多核就绪队列（全局 FIFO 或每 CPU + 工作窃取，演进由 profile 驱动）。

## 4. 关键语义（对齐 Linux）

| 操作 | 语义要求 |
|---|---|
| fork | 复制 PCB/TCB；子进程只继承调用线程；地址空间 CoW |
| exec | 替换地址空间与映像；保留/重置信号动作按 POSIX 规则 |
| exit/wait | 僵尸回收、exit code 传递、SIGCHLD 通知 |
| clone | 线程创建（共享地址空间/FD/信号）与进程创建（独立） |
| 信号 | sigaction/signal 语义、挂起队列、mask、rt_sigreturn；SIGKILL/SIGSTOP 不可捕获 |
| futex | 用户态同步原语（CAgent 高频） |
| 多核 | 每 CPU 就绪队列；迁移与唤醒路径要避免 thundering herd |

## 5. 性能关注点（BuildStorm 相关）

- 上下文切换开销：最小化保存/恢复面。
- 唤醒路径：事件驱动（阻塞任务注册 Event，事件触发入队），避免轮询。
- 多核伸缩：就绪队列锁竞争（参考一等奖：8 核加速比 RV 1.67× / LA 1.68× 为达标参考）。
- 信号与 syscall 返回路径：trap 入口/返回热路径（一等奖普遍在此做缓存优化）。

## 6. 验收标准

1. fork/exec/wait/exit 基础测例通过（含错误路径与 errno）。
2. 信号测例通过：捕获/忽略/默认、mask、pending、rt 信号队列、SIGKILL 优先级。
3. 管道与 futex 并发测例通过；无丢唤醒（一等奖教训：pipe 等待队列丢唤醒为典型 bug）。
4. SMP 测例通过：多核同时 fork/调度无死锁（配套 lockdep/spinlock 检测）。
5. 双架构上下文切换一致（上下文结构体差异收敛在 arch 层）。
6. 基准：fork+exit 循环、上下文切换延迟、唤醒延迟纳入性能台账（证据记录）。
