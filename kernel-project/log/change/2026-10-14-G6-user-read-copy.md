# G6：用户读取路径逐页校验

- 日期：2026-10-14；分类：小迭代。
- 关联 Boundary：`log/boundary/G6-user-read-copy.md`。
- 子系统：mm、syscall、用户测试。

## 问题与改动

此前 `copy_from_user` 只检查虚拟地址数值区间，直接构造指向用户内存的 `'static` 切片。对区间内未映射页，内核可能发生致命缺页；`'static` 还错误地暗示数据在进程退出或 exec 后继续有效。

现在由 `MemorySet::copy_from_user` 先逐页验证 USER|READ PTE，再经物理页身份映射复制为内核 `Vec<u8>`。syscall 层保留 ABI 地址边界与 EFAULT 映射；`write` 单次最多一页，避免用户长度导致巨大堆分配（允许短写）。C 字符串按页读取，遇 null 即停止，不再要求字符串后面未使用的页面也映射。C8 定向用例检查“位于用户范围内但未映射”的指针返回 EFAULT，随后程序继续运行。

## 验证

| 检查项 | 命令 | 结果 | 验证者 |
|---|---|---|---|
| RISC-V COW 构建 | 比赛容器内 `make build-cow` | PASS；e_entry=0x80200000 | agent-run |
| LoongArch 构建 | `/tmp` 临时源码副本、比赛容器内 `make ARCH=loongarch64 defconfig && make build` | PASS；e_entry=0x200000；临时副本已删除 | agent-run |
| 格式 | 比赛容器内 `make fmt-check` 及用户程序 `rustfmt --check` | PASS | agent-run |
| C8 运行验收 | 比赛容器内 `make run-cow RUN_ID=20261014-cow-final` | PASS；未映射指针返回 EFAULT，`G6-COW-PASS` | agent-run |
| shell 组合回归 | 比赛容器内 `make test-shell RUN_ID=20261014-shell-regression` | PASS；`shell: done`；串口有 7 个文件名、`hello ramfs`、退出码 7/127、`cat: open failed` | agent-run |

## 收口扫描与剩余风险

- 用户页只由 MemorySet/PTE 持有；返回 Vec 由 syscall 调用者拥有，不再泄漏用户页借用。
- 未改 syscall 编号与参数。大 `write` 可短写，是既有 syscall 允许的语义；用户程序须处理部分写入。
- 用户缓冲复制中的堆 OOM 仍取决于全局分配器；当前各调用方中，`write` 限一页、路径/参数限固定长度。
- `wait4` 回收顺序、`read` 共享偏移提交顺序属于跨 owner 事务，本次不改，需单独设计评审。
- `test-shell` 是新增的有界运行入口，pattern 仅承诺到达 `shell: done`；上述详细输出来自本次 serial.log 人工核对，尚未编码成独立断言。
