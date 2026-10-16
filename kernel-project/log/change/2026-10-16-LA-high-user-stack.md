# LoongArch 高地址用户栈与 TLB refill 层级修正

- 日期：2026-10-16；分类：架构页表局部迭代；关联边界：`log/boundary/LA-high-user-stack.md`。

LoongArch 的 `PWCL` 定义 PT/Dir1/Dir2 三级页表，公共页表按根/中间/叶建立；
refill 旧序列 `lddir 3/1` 与此配置不一致。现改为 `lddir 2/1`，按两级目录依次读取，
首任务用户栈顶恢复为 `0x7f00_0000`，与 RISC-V 一致。没有新增第二份页表或栈布局状态。

## 验证与诊断

- agent-run：探索序列 `lddir 3/2/1` 的 `test-la-pipe RUN_ID=20261016-la-highstack-pipe-a` 超时，串口停在 `activating paging`，已从最终代码移除。
- agent-run：最终 `lddir 2/1` 下，LoongArch `test-la-pipe RUN_ID=20261016-la-highstack-pipe-b` PASS。
- agent-run：LoongArch `test-la-process RUN_ID=20261016-la-highstack-process` PASS，含用户栈写入和 fork/COW。
- agent-run：LoongArch `test-la-exec RUN_ID=20261016-la-highstack-exec` PASS，含新栈 argv。
- agent-run：LoongArch `test-la-dir RUN_ID=20261016-la-highstack-dir` PASS。
- RISC-V 代码路径未改；本轮之前 `test-pipe RUN_ID=20261016-la-pipe-rv-regress` PASS。

本次仅验证低半地址空间内的 `0x7f00_0000` 用户栈；不意味着高半内核映射、ASID 或时钟抢占完成。
