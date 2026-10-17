# RFC：LoongArch 同步父子进程闭环

- 日期：2026-10-15；状态：同步父子闭环固定范围已运行验证。

## 决策

沿用共享 `kernel::task` 的 fork/exit/wait4 状态机，不新建 LoongArch 私有调度器。
LoongArch 的页表、trap 和切换汇编继续留在 `arch/loongarch64`。进程切换暂由父任务阻塞等待子退出触发，时钟抢占尚未启用。

用户态验收程序检查 fork 的父子返回值、子退出码、父 wait4 状态写回，并在成功时输出唯一标记。COW 的用户写故障由公共 `MemorySet` 处理、架构层负责刷新 TLB。

原 0x7f00_0000 用户栈在首次存储时触发 LoongArch PIS(ECODE=2)，即使映射两页也未消除；
将首任务栈放到 0x1001_0000 后，初始写入、父子 COW 及 wait4 均通过。这是定向布局，
高虚拟地址页表遍历的根因仍待查，不可据此宣称任意用户地址均可用。

## 风险

- LoongArch trap 现场与切换现场必须分别位于各任务内核栈，SAVE0 在返回用户态前更新。
- COW 清写后内核写用户页必须先复制，不能直接修改共享物理页。
- wait4 写回失败不得提前回收子任务；最后任务退出时不能释放当前内核栈。

## 后续

此闭环通过后仍需单独验收 exec、时钟抢占、FD/VFS ABI、多代 COW 与故障注入，才能接近 RISC-V 当前进度。

## 验证证据

- agent-run：`test-la-process RUN_ID=20261015-la-process-final-b` 命中 `LA_USER_EXIT_PASS code=0`；脚本按顺序检查 fork、切换、子 exit、父 wait、`LA_PROCESS_PASS`，串口含子/父各一次 `[cow] copy`。
- agent-run：LoongArch `test-user RUN_ID=20261015-la-process-user-regress`、`test-la-switch RUN_ID=20261015-la-process-switch-regress` 通过。
- agent-run：RISC-V `test-process RUN_ID=20261015-la-process-rv-regress`、`run-cow RUN_ID=20261015-la-process-rv-cow` 通过；后者 128 轮回收检查通过。
