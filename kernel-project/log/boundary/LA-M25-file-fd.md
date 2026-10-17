# LoongArch M2.5 文件描述符首批接入边界

- 日期：2026-10-15；分类：小迭代；状态：固定范围运行验收通过。
- 目标：让 LoongArch 复用公共 VFS/FD 表，通过 `openat/read/close` 从 ramfs 和 staticfs 读取用户态文件。
- 非目标：本轮不接管已有 LoongArch `write` 控制台路径，也不宣称完整 FD ABI、阻塞控制台输入或时钟抢占。
- Owner：`kernel/fs` 拥有路径、节点和打开描述；`kernel/task` 拥有 FD 槽；`kernel/syscall/fd` 负责用户指针与 errno；架构层只负责串口读就绪探测。
- Handoff：syscall 逐页校验用户缓冲后交给 VFS；read 只在目的页已准备后推进文件偏移。
- Failure：坏路径返回 ENOENT，坏 FD 返回 EBADF，坏用户指针返回 EFAULT，且失败不消费文件偏移。
- Cleanup：`close` 释放 FD 槽中的 Arc；进程退出仍由 task owner 释放整个 FD 表。
- 保护契约：现有 RISC-V 文件 syscall、LoongArch 控制台 write/exec/fork，Linux `openat=56/read=63/close=57` 的参数与返回值。
- 验收：LoongArch 原生用户程序验证 ramfs/staticfs、EOF、坏 FD/EFAULT 后继续使用同一 FD；RISC-V VFS 与进程回归不退化。
- 停止条件：若需修改 VFS owner、已有 UAPI 编号或现有控制台 write 行为，停止并另立边界。

agent-run：比赛镜像 `test-la-file RUN_ID=20261015-la-file-a` 通过，见 `LA_FILE_PASS` 和最终退出码 0；LoongArch `test-la-exec`/`test-la-process` 及 RISC-V `test-vfs`/`test-console` 回归通过。
