# LoongArch M2.8 COW 分配失败验收边界

- 日期：2026-10-16；分类：内存失败路径局部迭代；状态：固定范围运行验收通过。
- 目标：在 LoongArch 启动自检中复用公共 COW 页帧故障注入，覆盖 fork 地址空间复制的每个物理帧分配点，并检查失败后可重试及资源回收；内核地址空间回收和真实用户任务 fork/wait4 均重复执行 128 轮。
- 非目标：不扩展 SMP 故障注入语义，不加入任意内核分配器故障注入，也不改变生产构建行为。
- Owner：页帧故障注入与 COW 状态由 `kernel/mm` 管理；LoongArch 测试目标选择原生 COW 压力 ELF。
- Handoff：启动自检先运行临时 MemorySet 的分配点 sweep/回收循环，再运行当前首任务 fork 早期失败检查，最后进入 128 轮用户态 COW 压力。
- Failure：任何资源计数泄漏、父映射改变、引用残留或用户态失败均使验收失败；仅专用测试 feature 编译故障注入代码。
- Cleanup：所有临时 MemorySet 和子空间在每个失败点结束时释放；注入 budget 在一次拒绝后自动撤销。
- 保护契约：默认 LoongArch 与 RISC-V 生产构建不启用注入；RISC-V 原 M2.8 测试语义不变。
- 验收：LoongArch 专用 QEMU 目标命中 M28 frame/COW/fork markers，并通过用户态 fork/COW/exit/wait。
- 停止条件：若 LoongArch 需要多核并发注入，则另立 SMP 测试策略，不复用单核全局 budget。

agent-run：比赛镜像 `make ARCH=loongarch64 test-la-cow-oom RUN_ID=20261016-la-cow-userstress-a` 通过，
覆盖 134 个 MemorySet 页帧分配失败点、128 轮内核地址空间回收、跨页 COW 写失败重试、task fork
早期 9 点及 128 次真实用户态 fork/COW/exit/wait4。RISC-V `make ARCH=riscv64 test-cow-oom
RUN_ID=20261016-la-cow-oom-rv-b` 仍通过（520 个地址空间故障点及用户态 128 轮回收）；
格式检查修正后双侧 `fmt-check` 通过。
