# LoongArch 高地址用户栈恢复边界

- 日期：2026-10-16；分类：架构页表局部迭代；状态：固定范围运行验收通过。
- 目标：修正 LoongArch 三级页表 TLB refill 的目录层级，并将首任务用户栈恢复至与 RISC-V 一致的 `0x7f00_0000`。
- 非目标：不实现高半内核映射、ASID、按需栈增长或时钟抢占。
- Owner：TLB refill 与 CSR 页表布局属于 `arch/loongarch64/mm`；栈顶常量属于 `arch/loongarch64/task`；MemorySet 继续唯一拥有用户映射。
- Handoff：公共页表按三层建立根/中间/叶；架构 refill 必须按同一索引顺序读取，不能在内核另建映射副本。
- Failure：QEMU 出现 PIS/PIL、fatal trap 或超时即不宣称高栈完成；只改本轮代码且保留证据。
- Cleanup：构建在隔离目录中进行，不更改现有内核/用户映像所有权。
- 保护契约：已有低地址用户 ELF、fork/COW/exec/FD syscall 与 RISC-V 页表行为不变。
- 验收：LoongArch 原生用户态写栈、fork/COW、exec 与文件/pipe 定向测试在高栈下通过；RISC-V VFS/pipe 回归不退化。
- 停止条件：若须改公共页表树、MemorySet owner 或 Linux ABI，先另立 RFC。

agent-run：第一次 `lddir 3/2/1` 探索在分页切换处超时，不作为完成证据；
改为 `lddir 2/1` 后，比赛镜像 `test-la-pipe RUN_ID=20261016-la-highstack-pipe-b`、
`test-la-process RUN_ID=20261016-la-highstack-process`、
`test-la-exec RUN_ID=20261016-la-highstack-exec`、
`test-la-dir RUN_ID=20261016-la-highstack-dir` 均通过。
