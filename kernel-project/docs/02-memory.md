# 02 · 内存管理设计

## 1. 目标

提供满足 Linux UAPI 语义（mmap/mprotect/brk/fork 的 CoW）且性能足够的地址空间管理。支撑 CAgent 内存类测例与 BuildStorm 的进程创建负载。

## 2. 组成

```
kernel/mm/
├── address.rs      强类型物理/虚拟地址、页号与对齐检查
├── heap_allocator.rs 内核全局堆（空闲链表、释放与相邻块合并）
├── kernel_space.rs 内核映像布局与内核页表所有权
├── kernel_stack.rs 任务内核栈的页帧所有权
├── frame_allocator.rs 物理页帧分配器与唯一所有权令牌
├── page_table.rs   公共三级页表遍历
├── memory_set.rs   MemorySet 与 MapArea 集合
├── maparea/        映射区类型：匿名 / 文件私有 / 文件共享 / ELF 段 / 栈 / 堆 / 共享内存
├── pagefault.rs    缺页处理（懒分配、CoW、文件页回读）
├── ubuf.rs / uptr.rs   用户指针安全访问（边界校验）
└── (可选) swap.rs      匿名页换出（feature: swap-memory）
```

M1 按依赖顺序落地：地址类型与链接布局 → RAM 发现 → 物理页分配器 → 双架构 PTE
编码/页表寄存器 → 公共页表遍历 → 内核映射与分页切换。分页切换前不得把物理直址
启动描述成已建立内核地址空间。

当前实现已完成内核身份映射与分页切换；高半直接映射、ASID 和用户地址空间仍未接入。
内核堆当前使用 1 MiB 固定 backing，支持 Rust `alloc` 类型；按页扩容与 slab/per-CPU
缓存属于后续性能阶段。

## 3. 核心抽象

- **AddrSpace**：per-process 地址空间，持有 `MapArea` 管理器与 `PageTable`，负责 ELF 装载、mmap、vDSO 映射、CoW。
- **MapArea 类型**：匿名映射、文件私有（COW 语义）、文件共享（MAP_SHARED）、ELF 段、用户栈/堆、共享内存区。每类有自己的 load/fault/teardown 语义。
- **页帧分配器**：全局物理页分配/释放；分配失败策略在内核堆与物理页分开处理（内核堆失败 panic 可接受，见 AGENTS.md 分配约束）。

## 4. 关键语义（对齐 Linux）

| 操作 | 语义要求 |
|---|---|
| mmap | 匿名/文件、私有/共享、固定/非固定地址；PROT/FLAG 校验与 errno 对齐 |
| mprotect | 权限变更 + TLB/页表同步；只读→写触发缺页保护 |
| brk | 堆增长；需考虑稀疏分配（按页懒分配） |
| fork | 子进程地址空间 CoW 复制；父写时缺页复制（fork 热路径优化点） |
| exec | 全新地址空间；旧映射释放；程序头一次性读取（热路径优化点） |
| 缺页 | 懒分配：首次访问才建页；文件映射按 offset 读回 page cache |
| vDSO | 固定映射到用户态；时钟/rt_sigreturn 走 vDSO 快路径 |

## 5. 性能关注点（BuildStorm 相关）

- 页表深度与 TLB 行为：尽量按需建立大页映射（若硬件支持）。
- 页分配器缓存：per-CPU 页缓存减少锁竞争。
- 零页共享：首次匿名写前共享零页，减少零初始化开销。
- fork/exec 热路径：减少页表复制量（lazy page table）、减少 ELF 解析重复 I/O。

## 6. 验收标准（M 阶段对应）

1. 匿名/文件 mmap 测例通过（含边界：零长度、溢出、未对齐、非法 flag）。
2. CoW fork 测例通过（子写不污染父、父写不污染子）。
3. 缺页懒分配与文件映射回读正确；压力下无泄漏（回归脚本校验）。
4. 双架构页表语义一致（RV 的 Sv39/Sv48 与 LA 的页面粒度差异收敛在 arch 层）。
5. 性能：fork 1000 次耗时纳入基准（与基线对比，证据记录）。
