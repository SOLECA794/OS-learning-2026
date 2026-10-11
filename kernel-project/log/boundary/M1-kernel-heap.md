# M1 内核堆边界

## 目标

建立 Rust 全局内核堆：从页帧分配器取得一段内存，支持带对齐的动态分配、释放和相邻空闲块合并，使 `Box`、`Vec`、`Arc` 等 `alloc` 类型可用。

## 非目标

- 不实现 slab、per-CPU cache、按页扩容或内存压力回收。
- 不把用户进程 `brk`/`mmap` 与内核堆混用。
- 不接入任务、文件系统或驱动对象。

## Owner 与 Handoff

- `kernel/mm/frame_allocator.rs` 提供堆的连续物理页 backing。
- `kernel/mm/heap_allocator.rs` 独占 backing，并拥有块切分、回收、合并和全局分配器实现。
- 其它内核模块只使用 Rust `alloc` API，不直接操作堆内部链表。

## Failure / Cleanup

- 重复初始化、零容量或页帧不足返回显式错误。
- 普通分配失败由 Rust allocation error handler 打印诊断并停机。
- 堆 backing 在内核生命周期内常驻，不允许运行期整体析构。
- 释放地址必须来自该堆且布局与分配时相同；这是 `GlobalAlloc` 调用方契约。

## 受保护契约

- 物理页分配器、内核页表与 trap ABI 不改变。
- 用户地址空间不映射或管理内核堆。

## 验收与验证

- 启动期以 `Box` 和 `Vec` 验证分配内容，并在释放后确认空闲字节恢复。
- `make all` 双架构编译、链接、镜像检查通过。
- 未经明确要求不启动 QEMU，因此本轮启动自检标记为 `not-run`。

## 停止条件

若固定 backing 无法覆盖当前 M1 基础对象，或需要并发中断上下文分配策略，则拆分扩容/并发设计阶段。
