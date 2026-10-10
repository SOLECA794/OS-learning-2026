# 2026-10-10 M1 内核分页激活

## 变更

- RISC-V64：写入 Sv39 `satp` 并执行全局 `sfence.vma`。
- LoongArch64：配置 PGDL、PWCL、STLBPS、TLBREHI、TLBRENTRY，安装无栈软件 refill 入口并切换 CRMD.DA/PG。
- 公共映射增加 Normal/Device 内存类型；LoongArch QEMU UART 与 GED 页使用强序非缓存映射。

## 验证

- `agent-run`：在 `zhouzhouyi/os-contest:20260510` 中执行 `make all`，两个架构均编译、链接和镜像检查通过。
- RISC-V64 ELF 入口保持 `0x80200000`；LoongArch64 ELF 入口保持 `0x200000`。
- `not-run`：未启动 QEMU；`mm: kernel page table active` 尚未获得运行期观测。

## 架构摩擦扫描

- 映射内容仍由 `MemorySet` 单一持有；架构层只接收根 PPN，不保存第二份页表状态。
- 设备内存属性通过窄的 `MemoryType` 传给 PTE 编码器，LoongArch MMIO 没有泄漏到公共映射策略。
- LoongArch refill 使用专用 `TLBRSAVE`，不依赖尚未建立的 per-core trap 栈，也不改变普通 trap ABI。
