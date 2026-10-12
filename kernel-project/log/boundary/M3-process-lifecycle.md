# M3 boundary：进程生命周期（fork / exit / wait，RV 普通复制）

日期：2026-10-12
前置：W3（bb5a3ae，上下文切换 + 时钟抢占）；规划书 W3-4「普通 fork、exit/wait」。

## 目标

用户程序 fork 出子任务：父子独立地址空间（普通复制，非 COW）、子 exit 后父 wait4 回收、
等待时序与资源回收正确。

## 实现边界（语义）

- memory_set.rs：`dup(&self, layout, memory_end)`——逐页复制地址空间（Framed 区域新建页 + 拷贝内容；
  Identity 内核映射区域跳过、重建一次）。
- task/mod.rs：TCB 增 state（Ready/Running/Waiting/Exited）、parent、exit_code、waiting_child；
  `sys_fork()`（复制地址空间 + 新内核栈 + 现场拷贝且 a0=0 + 入就绪队列）；`sys_exit(code) -> !`
  （Exited + 存储 code + 唤醒 waiting 父 + 调度切走）；`sys_wait4(pid, status_ptr)`（子已退出则回收并
  写 WEXITSTATUS 编码 status；未退出则 Waiting 阻塞，子 exit 唤醒后重查；无子返回 ECHILD）。
- schedule()：当前任务仅 Ready 才重新入队（Waiting/Exited 不再调度）；就绪队列空 → park（登记）。
- syscall/mod.rs：SYSCALL_FORK=220、SYSCALL_WAIT4=260；sys_exit 改走 task 模块（不再 park）。
- boot_tasks：改为单任务 T0（uapp 自行 fork，父子关系由此建立；双任务调度能力保留）。
- user/main.rs：默认模式改为 fork 验收程序（子写 3 次后 exit(42)；父 wait4 校验后 exit(0)）；
  fault-test 分支保留。

## 非目标（登记，后续排期）

- COW / lazy allocation（W7-8）；exec（依赖命名程序来源，W5 与 VFS 一起）；孤儿收养（无 init，登记限制）；
  SIGCHLD / 信号（远期）；SMP 调度；内核态抢占。

## 并发前提（登记）

同 M2：仅 bootstrap hart 运行内核；SpinLock 无中断屏蔽，内核态不抢占下安全。

## 验收（agent-run）

1. RV make build PASS。
2. QEMU 运行 5s：子写 3 个 c → exit(42)；父 wait4 得 42 → 打印 PASS → exit(0)；全任务退出、无 panic。
3. 内存不泄漏：fork 后 allocated 页数 = 回收后页数（frame allocator 计数对照）。
4. fault-test 回归 PASS；LA make build PASS（LA 路径未变）。
5. 稳定性复跑 3 次全 PASS。

## 残留风险（登记）

- 父先于子 exit → 子成孤儿无人 wait（无 init 收养）；W4 测试避免该时序，登记后续处理。
- wait4 阻塞为轮询重查（无独立 wait 队列）；唤醒粒度 = 任务级，多子场景后续优化。
- 就绪队列空（全员阻塞/退出）→ park 停机；后续事件机制接管。
- 共享 LOAD 页权限并集仍宽松（M1.6 逐页收窄）。