# M2 boundary：RV 单核上下文切换 + 用户态时钟抢占 RR

日期：2026-10-12
前置：W2（3e432cb，syscall errno 语义全绿）；docs/03 任务调度设计（远期，本节取最小子集）。

## 目标

两个用户任务（同一 ELF 两个实例）在无主动 yield 的情况下均能前进：时钟中断驱动抢占，
任务交替执行，上下文保存/恢复正确（RV 单核）。

## 实现边界（语义）

- 新 kernel/task/：TaskControlBlock（space: Box<MemorySet> + kstack: KernelStack + task_ctx）+ 全局 FIFO 就绪队列 + schedule()。
- arch/riscv64/task.rs：TaskContext（ra/sp/s0-s11，repr(C)）+ __switch（global_asm，布局与结构体共享 ABI）。
- arch/riscv64/timer.rs：SBI TIME 扩展（eid="TIME" fid=0 set_timer）+ sie.STIE + 10ms 周期（QEMU virt timebase=10MHz）。
- kernel/trap.rs：TrapCause::Interrupt{code=5}（STI）→ 重设定时器 + schedule。
- kernel/mod.rs：RV 路径 init_tasks（双任务，T0 restore 直进、T1 就绪）；LA 路径保持单任务 init_user 现状。
- user/src/main.rs：默认模式 _start 改自旋循环 + 周期写（fault-test feature 分支保留）。

## 非目标（登记，后续排期）

- fork/exec/wait/exit（W4）；内核抢占；SMP 调度与就绪队列锁（M3）；COW；LA 用户态调度；信号。

## 并发前提（登记）

M2 阶段仅 bootstrap hart 运行内核（从核在 kernel_main park），就绪队列与 TCB 无跨核竞争；
M3 SMP 落地时必须为就绪队列引入锁并审计中断屏蔽。内核态不抢占：trap 进入时 sstatus.SIE 被
硬件清零，handler 期间无嵌套中断，只有用户态（SIE=1）会被时钟打断。

## 验收（agent-run）

1. RV make build PASS；QEMU（-smp 1 -m 4G）运行 >=3s：日志出现双向 [sched] 交替，两任务均输出 tick。
2. fault-test 回归：make test-fault 仍 PASS（0x80200000 访问 → fatal trap from_user=true）。
3. 12x 快速回归（启动日志完整、无 panic）无崩溃。
4. LA make build PASS（LA 路径未变，编译回归）。

## 残留风险（登记）

- 就绪队列无锁（单核前提，M3 SMP 换锁）。
- 时钟精度依赖 QEMU timebase 常量 10MHz（dtb 探测排期 M2 内存发现）。
- 任务退出路径未实现：W3 任务不 exit（死循环），W4 接入 exit/wait 后补回收。