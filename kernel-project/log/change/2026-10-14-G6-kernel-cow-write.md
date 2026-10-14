# G6：内核写回 COW 用户页

- 日期：2026-10-14；分类：小迭代。
- 关联 Boundary：`log/boundary/G6-kernel-cow-write.md`。
- 子系统：mm、syscall、user 验收程序。

## 问题与改动

原 `wait4`、`read`、`getdents64` 仅检查用户地址数值范围，然后直接写用户虚拟地址；没有逐页检查映射/权限，也没有在写入前解除 COW。现在 `MemorySet::copy_to_user` 先校验整个目标区间的用户可写或 COW 页，预先解除全部 COW，再经物理页身份映射逐页复制。syscall 层统一将失败转为 EFAULT；`wait4` 用字节写回，避免未对齐 `usize` 写入。

`uapp-cow` 的 `wait4` 改用非空的静态状态指针，增加 C6 断言（返回 pid 且状态等于 `5 << 8`）；`run-cow` 成功标记同步改为 C6。

后续 Patch：`read` 单次内核临时缓冲上限为一页（允许短读，后续调用继续使用共享偏移），避免用户提供巨大长度使堆分配失败；新增独立对齐页的 C7 用例，核对 `/hello.txt` 被内核读入仍处于 COW 的用户页。`run-cow` 成功标记提升到 C7，且 C7 同时要求 C6 成功。

## 验证

| 检查项 | 命令 | 结果 | 验证者 |
|---|---|---|---|
| RISC-V COW 构建 | 容器内 `make ARCH=riscv64 defconfig` 后 `make build-cow` | PASS；e_entry=0x80200000；含 C7 | agent-run |
| LoongArch 构建 | 容器内 `make ARCH=loongarch64 defconfig` 后 `make build` | PASS；e_entry=0x200000 | agent-run |
| 格式 | 容器内 `make fmt-check` | PASS | agent-run |
| 用户测试格式 | 容器内 `rustfmt --check --edition 2021 user/src/bin/uapp-cow.rs` | PASS | agent-run |
| C1–C8 运行验收 | 比赛容器内 `make run-cow RUN_ID=20261014-cow-final` | PASS；`G6-COW-PASS`，ELF SHA-256 `df837bf5babb…` | agent-run |
| 默认启动回归 | 比赛容器内 `make test RUN_ID=20261014-default-regression` | PASS；`MATCHED` | agent-run |
| 非 COW 缺页回归 | 比赛容器内 `make test-fault RUN_ID=20261014-fault-regression` | PASS；`fatal trap` 受控匹配 | agent-run |

## 残余风险与收口扫描

- `wait4` 在状态指针写入前已经回收子任务；若无效指针导致 EFAULT，子任务不会保留供重试。这是既有顺序问题，后续应把可写校验放在回收前。
- `read` 在用户复制前推进文件偏移；若映射无效导致 EFAULT，偏移不会回滚。后续应在文件读取前校验写目标。
- COW 分配失败会留下已经解除共享、但尚未写入的当前地址空间页；页面仍由 MemorySet 所有，无泄漏。用户数据尚未变化。
- COW 状态仍只有 MemorySet 一份真相；syscall 不读取 COW 集合。未增加 public API、ABI 或临时桥。
- 首次 C7 运行失败（run-id `20261014-023428` / `20261014-cow-diagnose`）：定向程序误按标准 Linux `openat(dirfd,path,...)` 传参；本项目当前约定 `a0=path`，与现有 shell 相同。修正测试参数后 C7/C8 通过。为防止 `C8-errno-fail` 误触发成功，运行器改匹配仅成功分支会输出的 `G6-COW-PASS`。
