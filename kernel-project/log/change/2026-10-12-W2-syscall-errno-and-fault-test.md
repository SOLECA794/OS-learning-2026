# W2：syscall 错误语义三连测 + 内核保护区访问受控（RV 全绿）

日期：2026-10-12
范围：W2 验收——规划书 W2「用户程序安全进入内核、错误指针受控、用户不能访问内核保护区」的 RV 落地。
前置：M1.5（aab10dc，用户态首闭环）已验证。

## 目标

uapp 默认模式三连测全部通过（exit(0)）；fault-test 模式用户态访问 0x80200000 触发受控停机（fatal trap from_user=true）。

## 实现边界（boundary W2 的落地）

- `linker/riscv64.ld` / `linker/loongarch64.ld`：boot 栈 8K 到 64K（每核）。
- `src/arch/{riscv64,loongarch64}/boot.rs`：栈尺寸计算同步 65536。
- `user/src/main.rs`：默认模式三连测（write 21 字节 / NULL 指针 EFAULT / fd=2 EBADF，全符 exit(0)）；fault-test 模式读 0x80200000（black_box 防优化）。
- `src/kernel/syscall/mod.rs`：EBADF 返回符号修正（-EBADF 改为 EBADF）。
- `src/kernel/trap.rs`：fatal trap 日志加 sp 字段。
- `src/arch/riscv64/trap/context.rs`：新增 sp() 访问器。
- `src/kernel/mm/mod.rs`：configured_memory_bytes 转 pub(crate)。
- `scripts/run-test.sh`：参数化 pattern/标签。
- `Makefile` / `Cargo.toml` / `user/Cargo.toml`：test / test-fault 目标与 uapp-fault-test feature 接线。
- `log/boundary/M1.5-usermode.md`：M1.5 边界文档（补提交）。

## 根因链（按发现顺序）

1. **uapp 断言 17 到 21**：msg 真实长度 21 字节（hello from user mode + 换行），断言 17 必失败。反汇编确认 0x15 已编译进 ELF。
2. **boot 栈溢出**：M1.5 `MemorySet::new_user` 调用链栈深超 8K，下垂进未映射空洞。修：栈 64K。
3. **EBADF 符号写反**：`const EBADF: isize = -9` 已是负数，返回处 `-EBADF` 得 +9，三连测 r3 检查失败退出码 1。syscall 诊断插桩一次定位。
4. **fault-test 读被优化**：`let _v = unsafe { *ptr };` 结果未用，load 被 rustc 删除，永不 fault 导致 qemu timeout。修：`core::hint::black_box`。
5. **-smp 4 异常归因**：根因是 build-fault 后产物残留（uapp.elf 被 fault-test 版覆盖），非 SMP 逻辑缺陷。干净重建后 -smp 4 正常。
6. **WSL 非交互 bash PATH 缺 cargo**：`wsl -- bash` 不读 ~/.bashrc，需显式 export PATH 加 ~/.cargo/bin。

## 验证矩阵（agent-run，WSL Ubuntu-24.04 + QEMU 9.2.1）

| 项 | 结果 |
|---|---|
| RV make build（默认 uapp） | PASS，e_entry=0x80200000 |
| 三连测（-smp 1 -m 4G） | PASS：hello 到 exited code 0 |
| fault-test（-smp 1 -m 4G） | PASS：fatal trap bad_addr=0x80200000 from_user=true |
| make test（-smp 4 -m 1024M，默认 pattern） | PASS |
| make test-fault（pattern fatal trap .*from_user=true） | PASS |
| 手动 -smp 4 -m 4G | PASS：hello 到 exited code 0 |
| 12 次 -smp 1 回归 | 12/12 PASS（exited code 0） |
| LA make build | PASS（栈 64K 同步，双架构编译） |

## 残留风险（登记）

- 用户态 page fault 统一走 fatal trap（无 SIGSEGV 语义）；M2 用户任务建立时区分并转信号。
- copy_from_user 仍以段边界校验为准；M1.6 换逐页 PTE 权限查询。
- 共享 LOAD 页权限并集（R+W+X）宽松；M2 逐页收窄。
- LA 用户态仍为占位 stub（W9-10 排期）。
- make test 默认 pattern 只查 M1 标记；严格退出码验证靠手动回归（或后续脚本增强）。

## Architecture Friction Scan

Safe：无阻断性摩擦。Euclid（记录不修）：configured_memory_bytes 依赖 KERNEL_MEMORY_MB 构建期注入，与 QEMU -m 参数两份表述无运行时校验；M2 内存发现阶段统一收口。