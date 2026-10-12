# W4：进程生命周期 fork / exit / wait4（RV，普通复制）

日期：2026-10-12
范围：规划书 W3-4 第二部分——普通 fork、exit/wait（exec 依赖命名程序来源，随 VFS 至 W5，见 boundary 非目标）。
前置：W3（bb5a3ae）；边界文档 log/boundary/M3-process-lifecycle.md。

## 目标

用户程序 fork 出子任务：父子独立地址空间（普通复制）、子 exit 后父 wait4 回收、等待时序与资源回收正确。

## 实现（boundary M3 落地）

- memory_set.rs：`dup(&self, layout, memory_end)`——Framed 区域逐页分配新物理页 + 拷贝内容；
  Identity 区域（内核映射）跳过、重建一次。
- task/mod.rs：TaskState（Ready/Running/Waiting/Exited）；TCB 增 parent/state/exit_code；
  `fork_current()`（dup 空间 + 新内核栈 + 父现场副本且 a0=0 + 入队）；`exit_current() -> !`
  （Exited + 存储 code + 唤醒 waiting 父 + 调度切走）；`wait4_step()`（回收 / WouldBlock / NoChild）；
  `block_current_and_wait()`（Waiting + schedule）。schedule 入队条件改 Ready|Running（Running 被抢占回 Ready），
  Waiting/Exited 不再调度；就绪队列空 → park（登记）。
- syscall/mod.rs：SYSCALL_FORK=220、SYSCALL_WAIT4=260；sys_exit 改走 task（不再 park）；
  copy_to_user + WEXITSTATUS 编码（(code&0xff)<<8）；WNOHANG 支持。LA 构建门控（task 引用 RV-only）。
- user/main.rs：默认模式改 fork 验收（子写 3 次 → exit(42)；父 wait4 校验 → PASS → exit(0)）；
  syscall 封装扩为 6 参数；fault-test 分支保留。
- boot_tasks：单任务 T0（uapp 自行 fork 建立父子；双任务调度能力保留）。

## 根因链（按发现顺序）

1. **schedule 入队条件（决定性根因）**：T0 直进用户态 state=Running，时钟抢占进 schedule 时 Running 不入队，
   队列空 → park。修复：Ready|Running 都重新入队（Running 转 Ready）。表现「一进用户态就 ready queue empty」。
2. **fork 返回值类型**：`fork_current().unwrap_or_else(|err| err)` 闭包返回 isize 与 Result<usize, isize> 冲突（E0308）。
   修复：match Ok/Err 分开。
3. **借用冲突**：`sched.tasks[sched.current].as_mut()` 索引与可变借用冲突（E0502）——先取 current 局部。
4. **LA 构建门控**：syscall 引用 kernel::task（RV-only 模块）→ LA E0433。修复：exit/fork/wait4 arm 与 sys_wait4 加
   #[cfg(feature="arch-riscv64")]，LA 回落 ENOSYS。

## 验证矩阵（agent-run，WSL Ubuntu-24.04 + QEMU 9.2.1）

| 项 | 结果 |
|---|---|
| RV make build | PASS，e_entry=0x80200000 |
| 主链路 5s | PASS：fork 0→1，子写 ccc → exit(42)，父唤醒 wait4 回收 → PASS → 父 exit(0)，全员退出 park |
| 内存回收（帧计数） | PASS：fork 后 1815 帧，wait 回收后 1292 帧（-523 完全对称，无泄漏） |
| 稳定性 ×3 | PASS（3/3 pass=1 fatal=0） |
| -smp 4 | PASS（从核 park 不干扰） |
| fault-test 回归 | PASS |
| LA make build | PASS（e_entry=0x200000，LA 路径未变） |

## 帧计数分析（W4 关键证据）

- 基线 513（内核页表树 + 3GB identity 映射一级页）+ heap 256（1MB）= 769。
- create_task 增量 523：用户空间 ~519（含每地址空间独立的 3GB identity 内核映射页表树 ~512 帧）+ 内核栈 4。
- fork 增量 523、wait 回收 -523：完全对称 → 无泄漏、回收正确（验收项 3 PASS）。
- 登记优化项：每任务独立页表树重复 3GB 内核映射（~512 帧/任务）——Linux 用内核映射页表共享消除；
  M 系列排期（SMP 前或 COW 阶段）。

## 残留风险（登记，boundary M3）

- 父先于子 exit → 孤儿无人 wait（无 init 收养），测试避免该时序。
- wait4 阻塞为任务级保守唤醒（任一子 exit 唤醒父，父重查后决定继续阻塞），多子场景后续优化。
- 就绪队列空（全员阻塞/退出）→ park；后续事件机制接管。
- 用户页权限仍宽松（merged 并集，M1.6 逐页收窄）。

## Architecture Friction Scan

Safe：无阻断性摩擦。Euclid（记录不修）：wait4 阻塞为轮询重查（无独立 wait 队列），SIGCHLD/信号阶段重构；
schedule 每次切换重新拿锁读 root_ppn（SMP 改每 CPU 队列时一并优化）。