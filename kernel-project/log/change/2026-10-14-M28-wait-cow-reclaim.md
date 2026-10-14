# M2.8：wait4 失败重试与 COW 回收修复

- 日期：2026-10-14
- 分类：Patch（恢复既有进程等待、地址空间所有权及回收行为，不迁移系统调用 ABI）
- 验证者：agent-run
- 环境：比赛镜像 `zhouzhouyi/os-contest:20260510`，QEMU RISC-V virt，4 harts / 1024 MiB；仍只有 bootstrap hart 执行任务。

## 根因与修复

1. `sys_wait4` 原先先释放子任务，再写回状态，EFAULT 后无法重试。现在 task 层只提供退出快照，syscall 完成写回后由 task 层提交回收。匹配任意子任务时扫描全部候选，优先返回已退出者。
2. `MemorySet::dup_cow` 原先没有登记子空间的 Framed 区域，导致 Drop 漏释放用户页，子进程再次 fork 也丢失用户映射。现在登记完整区域，并为所有共享页（包括只读代码页）增加引用。
3. 部分页表构造失败时，仅回滚当前区域已映射的前缀；此前完成的区域由新空间 Drop 清理。先完成所有子页表构造再提交父 COW 权限并刷新 TLB；内核栈分配也移到父空间修改之前。
4. 原用户测试的总 PASS 没有包含 C1–C4。现将父侧断言和子侧退出码纳入总结果，增加两次 EFAULT 后重试、重复等待返回 ECHILD、8 轮回收和子/孙多代 COW 隔离。

涉及代码：`kernel/task/mod.rs`、`kernel/syscall/mod.rs`、`kernel/mm/memory_set.rs`、`user/src/bin/uapp-cow.rs`。`make run-cow` 新增 `tools/check_cow_reclaim.py`，同时验证成功标记和八轮回收的物理帧计数，不只依赖打印 PASS。

## 验证证据

以下命令均在比赛镜像中经 make 执行；最终恢复普通 RISC-V 内核 ELF。

| 命令 | 结果与证据 |
|---|---|
| `make ARCH=riscv64 run-cow RUN_ID=20261014-wait-before`（仅新增测试，修复前） | 预期 FAIL：`WAIT-RETRY-FAIL`，重复回收 frames 从 1305 增至 1312 |
| `make ARCH=riscv64 run-cow RUN_ID=20261014-wait-final` | PASS：`WAIT-RETRY-PASS`、`G6-COW-PASS`；8 轮 frames 均为 1301，多代 fork 断言通过 |
| `make ARCH=riscv64 test-process RUN_ID=20261014-wait-final` | PASS：`process-20261014-wait-final/result.json` |
| `make ARCH=riscv64 test-vfs RUN_ID=20261014-wait-final` | PASS：`vfs-20261014-wait-final/result.json` |
| `make ARCH=riscv64 test-shell RUN_ID=20261014-wait-final` | PASS：`shell-20261014-wait-final/result.json` |
| `make ARCH=riscv64 test-console RUN_ID=20261014-wait-final` | PASS：`console-20261014-wait-final/result.json` |
| `make fmt-check` / `git diff --check` | PASS |
| `make ARCH=riscv64 build` | PASS，入口 `0x80200000` |
| 容器临时副本依次 `make clean`、`make ARCH=loongarch64 defconfig`、`make ARCH=loongarch64 build` | PASS，入口 `0x200000`；仅构建，无 LoongArch 用户态运行结论 |

运行证据均在 `artifacts/runs/` 下，对应 result.json 记录 ELF SHA-256 和运行参数。

## 边界与下一步

- 状态真相仍是 task 的任务槽；退出快照不拥有资源。观察、用户写回、提交之间不调度，当前单核非抢占内核中不会出现第二个回收者。SMP/同进程多线程落地时必须重新设计等待认领协议。
- 保留原有 `usize` 状态宽度、PID 选择及 options 子集，不宣称 Linux wait4 完整 ABI。任意子进程扫描修复已代码审查，尚无“第一个运行、后一个退出”的独立调度定向用例。
- 帧计数稳定只证明本固定输入没有逐轮物理帧泄漏，不证明任意工作负载无泄漏；任务槽 Vec 和单调 ID 仍会增长，堆耗尽仍按现有策略 panic。
- 物理帧分配失败的清理顺序已修复并代码审查，尚未做逐分配点 OOM 注入，不能宣称 M2.8 完整验收。
- 只读共享引用、Framed 区域和 COW 集合分别承担生命周期、映射范围和写时复制资格，未引入新的资源 owner；页表不再存在脱离区域所有权的成功构造结果。
- 下一步优先补 OOM 注入与长期资源压力测试，再处理标准 ABI 缺口；LoongArch 用户态仍未落地。
