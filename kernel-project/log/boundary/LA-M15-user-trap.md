# LoongArch M1.5 用户态闭环实施边界

- 日期：2026-10-15
- 分类：RFC（trap 保存协议、ELF 架构接受规则及用户态交接）
- 状态：M1.5 单任务范围已运行验收；不代表 M2/M2.8 已追平 RISC-V

## 语义边界

- **目标**：在 LoongArch QEMU 中运行原生 ELF64 用户程序，trap 从 PLV3 切至独立内核栈，完成 `write(1/2)` 和 `exit` 的定向闭环。
- **非目标**：本轮不实现 LA 调度、fork/exec/wait、FD/文件 syscall、时钟中断、用户态 COW 或 LTP 兼容；这些仍属于后续追平检查点。
- **Owner**：`arch::loongarch64::trap` 拥有寄存器/CSR 保存恢复；`kernel::mm` 拥有用户地址空间；`kernel::syscall` 拥有最小 ABI 策略；构建层拥有 LA 用户 ELF。
- **Handoff**：汇编将完整 `TrapContext` 交给公共 `kernel_trap_handler`；syscall 从现场取号/参，用户数据由当前单任务 `MemorySet` 校验复制。
- **Failure**：异常来源不符、非法用户指针或未知 syscall 返回 errno 或打印 fatal trap 停机；不得把内核栈错误静默伪装成用户程序完成。
- **Cleanup**：单任务退出后停驻；`MemorySet` 与内核栈在整个运行期间保留。M2 task owner 落地时删除这一临时单任务桥。

## 受保护契约与验收

- RISC-V `test-vfs`、`test-process`、`test-shell` 等既有语义不变。
- LoongArch 的 `TrapContext` 字段偏移必须与汇编一致；内核来源 trap 不能错误切到用户内核栈。
- 用户 ELF 必须为 `EM_LOONGARCH=258`，RISC-V ELF 仍仅由 RV 路径接受。
- 验收：比赛镜像 `make ARCH=loongarch64 test-user` 在 QEMU 看到用户态写入标记及受控退出；`make ARCH=riscv64 test` 不回归。
- 停止条件：需要改变既有 Linux syscall 号/参数、修改 M2 task owner、无法证明 trap 栈切换安全，或构建产物不具备目标架构身份。

下一阶段：将单任务地址空间/栈所有权迁入共享 task，再移植 LA 时钟中断和 `TaskContext` 切换；这不是本轮已完成的能力。
