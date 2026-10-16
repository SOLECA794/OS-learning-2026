# LoongArch QEMU virt 串口输入唤醒边界

- 日期：2026-10-16；分类：架构中断局部迭代；状态：QEMU virt 有界验收通过；实板仍未实现。
- 目标：让 QEMU virt NS16550A RX 中断进入 LoongArch 内核，缓存输入并唤醒等待中的 stdin 任务。
- 非目标：不支持 Loongson 实板中断控制器、不实现 SMP/IPI、不修改 UART TX 路径。
- Owner：QEMU PCH PIC/EXTIOI/CPU CSR 路由由 `arch/loongarch64/interrupt` 管理；UART RX FIFO 与字节队列由 `arch/loongarch64/console` 管理；等待状态和唤醒由 `kernel/task` 管理。
- Handoff：trap 将 timer/external 位图交给内核策略；外部中断只排空 UART、确认中断并唤醒等待者，不分配内存、不访问用户指针。
- Failure：未知 LoongArch 中断位继续 fatal；只在专用 QEMU `virt` 平台配置控制器，不推断实板寄存器。
- Cleanup：stdin 任务退出后由 task owner 回收；固定大小 RX ring 满时丢弃新字节并累计计数。
- 保护契约：RISC-V PLIC/SBI 路径不变；默认用户 syscall 语义不变；内核 idle 开中断仅包围等待指令并在返回时关闭。
- 验收：有界 QEMU 注入固定串口 fixture，证明读任务阻塞、CPU idle 后由 UART RX IRQ 唤醒并读到数据。
- 结果：`make ARCH=loongarch64 test-la-console` 观察到读者等待、idle、UART 唤醒、读取 `abc\n`、父任务 wait4 成功并以 0 退出；QEMU virt 4-hart 启动但目前只由 bootstrap hart 执行内核任务。
- 停止条件：若当前比赛镜像 QEMU virt 的 PCH/EXTIOI 连接与常见 QEMU 拓扑不一致，停止写寄存器并按镜像版本核对平台定义。

参考：QEMU LoongArch `virt` 设备定义（PCH PIC/UART IRQ）与 LoongArch PCH PIC/EXTIOI 实现；Linux PCH PIC 驱动中的寄存器偏移仅用于交叉核对。最终行为以比赛镜像 QEMU 运行验收为准。
