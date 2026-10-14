# 双架构 MemorySet 映射与回收自检边界

- 分类：小迭代；日期：2026-10-14。
- 目标：启动期在独立、未激活的 MemorySet 中建立两页 Framed 用户映射，核对虚拟页翻译与权限读回、重叠拒绝、撤销映射及页帧回收。
- 非目标：LoongArch 用户态切换、TLB 硬件行为、COW fork、mmap 系统调用、修改页表格式。
- Owner：MemorySet 拥有 MapArea 与数据页；PageTable 拥有中间表页；架构 PTE 后端只提供现有编码/解码。
- Handoff：`mm::init` 在分页激活与 heap 初始化后调用；自检不激活临时页表、不把令牌交给任务。
- Failure：断言失败在 M1 标记前停机；未通过不能声明映射能力已验证。
- Cleanup：先 remove 释放数据页，再 Drop 释放页表；最终 `allocated_count` 恢复调用前值。
- 受保护：当前内核页表、各架构启动流程、现有 MemorySet/FrameAllocator API 和用户态地址空间。
- 验收：比赛镜像中 RISC-V/LoongArch 各自 `make test` 到自检与 M1 标记；RISC-V VFS/COW 回归；格式与差异检查。
- 停止：若需要修改架构页表编码或影响当前活跃内核页表，停止并报告。
