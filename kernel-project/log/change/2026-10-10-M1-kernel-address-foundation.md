# 2026-10-10 M1 kernel/mm 地址基础

## 变更

- 新增强类型地址与页号，避免后续把物理地址、虚拟地址和页号混用。
- 新增 `KernelImageLayout`，统一读取链接脚本导出的内核段边界。
- 启动阶段校验各段顺序并打印布局，同时明确分页仍未启用。
- 为 RISC-V64 与 LoongArch64 链接脚本补充 text、rodata、data 边界符号。

## 决策理由

当前还没有可靠的双架构物理内存描述：RISC-V 后续应解析 DTB，LoongArch 还需接入平台内存图。此时硬编码 RAM 上界会把 QEMU 配置误当作通用架构事实。因此本轮先建立不依赖容量假设的地址与映像布局基础。

## 验证

- 在 `zhouzhouyi/os-contest:20260510` 中执行 `make all`：PASS。
- RISC-V64 ELF：`machine=riscv64`，入口 `0x80200000`。
- LoongArch64 ELF：`machine=loongarch64`，入口 `0x200000`。
- 未启动 QEMU；运行期的布局日志与断言仍需下一次显式启动验证。
