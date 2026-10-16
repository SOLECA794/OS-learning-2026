# LoongArch 单核时钟抢占边界

- 日期：2026-10-16；分类：架构中断局部迭代；状态：固定范围运行验收通过。
- 目标：LoongArch 用户态收到周期定时器中断后，交给公共 task 调度器切换任务。
- 非目标：不接 UART 外部中断、IPI、多核调度或精确时间计量。
- Owner：计时器 CSR 与中断确认归 `arch/loongarch64/timer`；中断策略在公共 `kernel/trap`；任务状态仍由 `kernel/task` 唯一维护。
- Handoff：架构上下文提交 ESTAT.IS 位图；trap 只确认已启用的 timer bit，并只在用户态来源调用 `schedule`。
- Failure：未知中断保持 fatal；定时器确认不向公共层泄漏 CSR；QEMU 无子进程运行标记则不宣称抢占完成。
- Cleanup：周期定时器在本核持续运行；中断源在处理时清除，任务退出仍由现有 task owner 回收。
- 保护契约：RISC-V STI/SEI 分发不变，LoongArch 同步 syscall/fork/COW/exec 路径不变。
- 验收：LoongArch 原生用例中父进程无 syscall 忙等，子进程仍输出标记并退出；既有 LA 进程/exec/FD 与 RISC-V 进程回归不退化。
- 停止条件：若需改变公共调度 owner、内核可抢占性或多核中断协议，先另立 RFC。

agent-run：比赛镜像 `test-la-preempt RUN_ID=20261016-la-preempt-b` 中，父用户任务忙等时子任务被调度并退出；
`test-la-process RUN_ID=20261016-la-timer-process`、
`test-la-exec RUN_ID=20261016-la-timer-exec`、
`test-la-pipe RUN_ID=20261016-la-timer-pipe` 及 RISC-V
`test-process RUN_ID=20261016-la-timer-rv-process` 均通过。
