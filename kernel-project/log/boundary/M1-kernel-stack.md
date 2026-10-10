# M1 内核栈内存边界

## 目标

由内存子系统提供可回收的任务内核栈：一次获得连续、清零、16 字节对齐的地址范围，并由唯一所有权对象负责完整归还。

## 非目标

- 不实现 TaskContext、上下文切换、PCB/TCB 或调度器。
- 不创建用户栈，不进入用户态，不修改 trap frame。
- 本阶段不实现独立高半栈虚拟区与 guard page。

## Owner 与 Handoff

- `kernel/mm/frame_allocator.rs` 拥有连续物理页分配和回收。
- `kernel/mm/kernel_stack.rs` 拥有栈大小、地址计算和栈资源生命周期。
- 后续任务模块只能持有 `KernelStack` 并读取 `top()`，不能自行释放其中页帧。

## Failure / Cleanup

- 页帧不足或分配器未初始化时，构造返回 `FrameAllocError`。
- `KernelStack` 析构通过 `FrameBlock` 逐页归还；发布后的活动栈不得被析构。
- 连续分配暂时只使用未消费的 bump 区，不从离散回收链拼接伪连续范围。

## 受保护契约

- 已有单页 `FrameTracker`、页表和 trap ABI 不变。
- 内核当前身份映射策略不变。

## 验收与验证

- `make all` 双架构编译、链接、镜像检查通过。
- 静态确认构造失败不泄漏，析构完整归还，栈顶满足 16 字节对齐。
- 不启动 QEMU；当前没有任务 owner 可以安全持有并切换到新栈。

## 停止条件

如果实现要求改变内核虚拟地址布局、页表激活协议或 trap ABI，则拆分为单独阶段。
