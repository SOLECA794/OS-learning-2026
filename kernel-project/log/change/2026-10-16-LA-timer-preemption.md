# LoongArch 单核用户态时钟抢占

- 日期：2026-10-16；分类：架构中断局部迭代；关联边界：`log/boundary/LA-timer-preemption.md`。

LoongArch 架构层新增本核周期定时器 CSR 编程：ECFG.LIE[11] 使能 TI，TCFG 启动周期计数，
TICLR 在 trap 中清除电平中断。公共 trap 只在 LoongArch 用户态来源的 TI 中断调用现有
`kernel/task::schedule`，内核态 TI 只确认，不在内核阻塞路径二次调度。
计数常量尚未从平台频率校准，不对外承诺精确时间单位。

## 验证

- agent-run：`test-la-preempt RUN_ID=20261016-la-preempt-b` PASS；父用户任务无 syscall 忙等，日志依次出现 fork、`[sched] task 0 -> task 1`、`LA_PREEMPT_PASS` 和子任务退出 0。
- agent-run：`test-la-process RUN_ID=20261016-la-timer-process` PASS。
- agent-run：`test-la-exec RUN_ID=20261016-la-timer-exec` PASS。
- agent-run：`test-la-pipe RUN_ID=20261016-la-timer-pipe` PASS。
- agent-run：RISC-V `test-process RUN_ID=20261016-la-timer-rv-process` PASS。

## 剩余边界与摩擦扫描

计时器状态仅在架构 CSR，任务状态仅在公共调度器；未新建调度真相或扩大内核态抢占。
仍未实现 UART RX 唤醒、IPI/多核、时间频率发现，也未验收 RISC-V M2.8 同等级的
COW 故障注入与长期回收。这些不能从单核抢占用例推断为完成。
