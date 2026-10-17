# LoongArch execve 接入边界

- 日期：2026-10-15；分类：小迭代；状态：固定范围运行验收通过。
- 目标：复用公共 `execve`，在 LoongArch 验证失败保留旧进程、成功替换地址空间与现场并传递 argv。
- 非目标：时钟抢占、动态链接、完整 envp、任意高虚拟地址布局。
- Owner：`kernel::syscall::exec` 拥有 exec 语义，`kernel::task` 拥有当前任务空间；`arch` 仅提供现场寄存器与用户栈布局。
- Handoff：syscall trap 成功时以新现场返回；失败时旧现场只接收 errno。
- Failure：路径/ELF/内存错误均在新空间发布前返回；旧空间不变。
- Cleanup：新空间构造失败自动释放；成功激活新页表后由 task 替换并释放旧空间。
- 受保护契约：RISC-V execve 行为、Linux syscall 221 的参数与 errno、LoongArch fork/wait4 路径。
- 验收：比赛镜像 `make ARCH=loongarch64 test-la-exec`；随后 LoongArch 进程与 RISC-V exec 回归。
- 停止条件：需要修改 trap 现场布局、共享 task owner、或放宽成功/失败语义时停止上报。

agent-run：比赛镜像 `test-la-exec RUN_ID=20261015-la-exec-final` 通过，`LA_EXEC_PASS` 后 task 0 以 0 退出；`test-la-process RUN_ID=20261015-la-exec-regress` 与 RISC-V `test-process RUN_ID=20261015-la-exec-rv-regress` 通过。
