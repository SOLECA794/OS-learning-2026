# CONTRIBUTING

> 协作约定。队友与外部贡献者按本文件操作。

## 分支

- `master`：主线，随时可构建可测试。
- 个人分支：`<initial>-<topic>`（如 `yyw-vfs-refactor`），完成后 PR 合回 master。
- 禁止直接 push master；本地提交后 push 个人分支，再合并。

## 提交信息

格式：`<type>: <简述>`

type 取值：
- `G0`–`G6`：赛题里程碑
- `M0`–`M4`：模块里程碑
- `W1`–`W4`：周任务
- `docs`：文档
- `style`：格式化 / warnings 清理，不改逻辑
- `fix`：缺陷修复
- `feat`：新功能

示例：`G6: COW fork 完整落地 + 验收`

## 构建与测试

环境：WSL Ubuntu-24.04。在 `kernel-project/` 目录下：

```bash
# 首次或 clean 后必须重新生成 config
python3 tools/kconfig.py --defconfig config/defconfig-riscv64

make ARCH=riscv64 build        # 构建，0 warnings
make ARCH=riscv64 test          # 基础回归
make ARCH=riscv64 test-fault    # 缺页故障注入
make ARCH=riscv64 run-cow       # COW 128 轮回收
make ARCH=riscv64 test-shell    # shell 有界回归
make ARCH=riscv64 test-vfs      # VFS 回归
make ARCH=riscv64 test-cow-oom  # COW OOM 520 点
```

合并进 master 前必须 6 项全绿。

## 文档维护

每次改动：
1. 在 `kernel-project/log/change/<日期>-<描述>.md` 写本次改动记录（事实、验证结果、未决问题）。
2. 若涉及设计决策，在 `kernel-project/log/rfc/<topic>/index.md` 补 RFC。
3. 合并后在根目录 `CHANGELOG.md` 顶部加一条。
4. 里程碑完成后更新根目录 `ROADMAP.md`。

## 禁止

- `git add -A`：仓库含 6 个 submodule，必须显式路径 add。
- 提交 `artifacts/runs/` 下的 QEMU 产物。
- 改 submodule 目录（T2026*）内容。
