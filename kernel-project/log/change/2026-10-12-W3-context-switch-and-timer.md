# W3：上下文切换 + 用户态时钟抢占 RR（RV 单核，双任务验收全绿）

日期：2026-10-12
范围：规划书 W3-4 第一部分——上下文切换、用户态时钟轮转（先用户态抢占、内核暂不抢占）。
前置：W2（3e432cb）；边界文档 log/boundary/M2-scheduler-rv.md。

## 目标

两个用户任务（同一 ELF 两个实例）在无主动 yield 的情况下均能前进：时钟中断驱动抢占，
任务交替执行，上下文保存/恢复正确（RV 单核；-smp N 下从核 park 不参与）。

## 实现边界（boundary M2 落地）

- `src/arch/riscv64/task.rs`（新）：TaskContext（ra/sp/s0-s11，repr(C)，与汇编共享 ABI）+
  `__riscv64_switch`（global_asm，按 C ABI 调用，call 返回地址即 old.ra）+ trap_return_address()。
- `src/arch/riscv64/timer.rs`（新）：SBI TIME 扩展（eid="TIME"，fid=0）set_timer + sie.STIE + 10ms 周期（QEMU virt timebase=10MHz）。
- `src/kernel/task/mod.rs`（新）：TaskControlBlock（space: Box<MemorySet> + kstack: KernelStack + task_ctx）、
  SpinLock、全局 FIFO 就绪队列（VecDeque）、create_task / launch_first / push_ready / schedule / boot_tasks。
- `src/kernel/trap.rs`：TrapCause::Interrupt{code=5}（STI）→ 重设定时器 + schedule（RV cfg 门控，LA 回落 unhandled）。
- `src/kernel/mod.rs`：init_user 拆双 cfg 版本（RV=boot_tasks 双任务，LA=legacy 单任务）。
- `src/arch/loongarch64/trap/context.rs`：补 sp() 访问器（registers[3]，与 RV 对齐 fatal 日志）。
- `user/src/main.rs`：默认模式改无主动 yield 自旋循环（每 500_000 次写 tick），fault-test 分支保留。

## 根因链（按发现顺序）

1. **arch re-export 缺 task/timer**：arch/mod.rs 用显式 re-export 列表，新模块未挂 → E0433。
   且 LA 分支不能带 task/timer（LA 未实现），上层引用必须 RV cfg 门控。
2. **include_bytes 路径深一层**：src/kernel/task/ 下要 ../../../build/user/uapp.elf（原 init_user 在 src/kernel/ 是 ../../）。
3. **ELF 类型名**：elf::ElfInfo 实为 elf::Elf。
4. **schedule 持锁切走 → SpinLock 死锁（决定性根因）**：第一次时钟中断进 schedule，__switch 挂起当前
   执行流时 guard 未 drop；对方任务再进 schedule 拿锁自旋死锁，表现为「切换一次后再无输出」。
   修复：锁内只做队列/状态更新并取出 old/new 现场指针与页表，guard 释放后再 __switch；
   切回后重新拿锁恢复自己的页表。
5. **-smp 4 首测 fatal 为脚本顺序假象**：make test-fault 构建 fault-test 内核后未重建默认内核，
   T0 一进用户态读 0x80200000 → fatal（预期行为）。修正测试顺序后 -smp 4 正常。

## 验证矩阵（agent-run，WSL Ubuntu-24.04 + QEMU 9.2.1）

| 项 | 结果 |
|---|---|
| RV make build | PASS，e_entry=0x80200000 |
| 双任务 6s（-smp 1 -m 4G） | PASS：0→1 与 1→0 各 143 次（完美对称），tick 107，fatal 0 |
| 稳定性复跑 4s | PASS：91/91 双向，fatal 0 |
| -smp 4 运行 4s | PASS：61/61 双向，ticks 28，fatal 0 |
| fault-test 回归 | PASS（0x80200000 → fatal trap from_user=true） |
| LA make build | PASS（e_entry=0x200000，LA 路径未变） |

## 残留风险（登记）

- 就绪队列 SpinLock 无中断屏蔽：M2 单核（内核态不抢占）安全；M3 SMP 落地时需锁顺序审计 + 关中断。
- 时钟精度依赖 QEMU timebase 常量 10MHz（dtb 探测排期 M2 内存发现）。
- 任务退出路径未实现：W3 任务不 exit（死循环），W4 接入 exit/wait 后补回收。
- 内核态不抢占（规划书 W3 决策）：syscall handler 长路径不会被时钟打断。
- LA 用户态调度未实现（W9-10 排期）。

## Architecture Friction Scan

Safe：无阻断性摩擦。Euclid（记录不修）：schedule() 每次切换重复拿锁读 root_ppn，M3 SMP 改每 CPU 就绪队列
时一并优化；log_info 每次切换打印为诊断热路径开销，release 构建保留（对齐一等奖调度观测习惯）。