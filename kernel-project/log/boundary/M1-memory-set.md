# M1 MemorySet 实现边界

## 目标

交付尚未激活 MMU 的完整内存集合基础：物理页分配与归还、双架构 PTE 编码、公共三级页表遍历、映射区所有权，以及可创建、映射、查询、删除和销毁的 `MemorySet`。

## 非目标

- 不写页表根寄存器，不开启分页，不刷新 TLB。
- 不实现用户 ELF、`mmap`、缺页、CoW 或文件映射。
- 不建立最终高半内核直接映射。
- 不解析 RISC-V DTB，也不实现 LoongArch 固件内存图。

## Owner

- `kernel/mm` 拥有物理页生命周期、页表遍历、映射区和 `MemorySet` 语义。
- `arch/*/mm` 只拥有 PTE 位编码、页表层数/索引以及当前启动模式下的物理地址访问规则。

## Handoff

- 构建配置把 QEMU RAM 大小以 `KERNEL_MEMORY_MB` 传入 Rust。
- `arch::mm` 把平台当前可用的连续物理范围交给 `kernel/mm`。
- `kernel/mm::pagetable` 只通过统一的 `arch::mm::PageTableEntry` 操作架构 PTE。

## Failure

- 地址未对齐、范围重叠、重复映射、页帧耗尽均返回显式错误。
- 区域映射中途失败时，撤销已经建立的叶子映射并归还其数据页。
- 启动配置或链接映像越过声明 RAM 时立即 panic，不继续使用错误内存。

## Cleanup

- `MemorySet` 拥有所有 `MapArea`。
- `MapArea::Framed` 的叶子物理页由该区域负责归还。
- `PageTable` 独占根页表与中间页表，析构时递归归还。

## 受保护契约

- 两架构现有 `_start`、链接基址、trap ABI 与启动日志不得改变语义。
- `arch` 仍是唯一架构差异入口；通用页表和地址空间代码不得引用私有架构路径。
- 当前保持物理直址执行，不得把“页表对象已实现”描述成“分页已启用”。

## 临时桥

RAM 上界来自现有 QEMU `-m` 配置。保留原因是 RISC-V DTB 与 LoongArch 平台内存图尚未接入；行为边界是只接受当前连续低内存模型；移除条件是平台内存发现完成后改由平台层提供可用区间列表。

## 验收标准与验证

- `make all` 同时通过 RISC-V64 与 LoongArch64 构建和链接检查。
- ELF 入口与现有链接基址保持不变。
- 代码审阅能证明 framed 数据页、中间页表页和根页表各有唯一 owner，失败路径能释放叶子页。
- 不启动 QEMU；运行验证需要用户另行明确授权。

## 停止条件

- 若实现要求现在启用分页、改变 trap ABI，或无法确认 LoongArch PTE 编码，则停止并上报。
