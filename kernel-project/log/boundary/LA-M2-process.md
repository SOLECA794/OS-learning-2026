# LoongArch M2 进程闭环实施边界

- 日期：2026-10-15；分类：RFC；状态：固定范围 QEMU 验收通过。
- **目标**：复用共享 task 的 fork、exit、wait4 与 COW 路径，在 LoongArch QEMU 完成父子进程闭环。
- **非目标**：本轮不宣称与 RISC-V M2/M2.8 全部持平；exec、时钟抢占、console 阻塞、FD/VFS 用户 ABI 另行验收。
- **Owner**：`kernel::task` 持有进程与调度状态，`kernel::mm` 持有 COW，`arch/loongarch64` 只实现 trap/页表/切换现场。
- **Handoff**：fork 复制当前 trap 现场，wait4 阻塞父任务并切到子任务，子 exit 唤醒父任务。
- **Failure**：fork 分配失败返回 ENOMEM；无子任务返回 ECHILD；非法状态指针返回 EFAULT 且不回收子任务。
- **Cleanup**：父任务成功 wait4 后回收子任务空间与内核栈；最后任务退出时保留当前栈并停驻。
- **保护契约**：Linux syscall 编号/参数、RISC-V 已有进程路径、LoongArch 单任务 `write/exit` 不变。
- **验收**：LoongArch 有界 QEMU `test-la-process` 输出 `LA_PROCESS_PASS`；`test-user`、`test-la-switch` 与 RISC-V 进程回归不退化。
- **停止条件**：需要改 trap 上下文布局、页表 ABI、共享调度 owner 或开启异步中断时停止并另立 RFC。

路线：CKPT 1 接入同步 fork/wait 与 COW 缺页；CKPT 2 原生用户程序验证父子返回、退出状态及回收。

agent-run：比赛镜像 `make ARCH=loongarch64 test-la-process RUN_ID=20261015-la-process-final-b` 为 `MATCHED`；串口有父子两次 COW 复制、子退出 7、父 wait 回收、`LA_PROCESS_PASS` 和父退出 0。LoongArch `test-user`/`test-la-switch`，RISC-V `test-process`/`run-cow` 同轮回归通过。
