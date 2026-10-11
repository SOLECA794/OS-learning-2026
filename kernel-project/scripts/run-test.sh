#!/usr/bin/env bash
# 最小运行测试入口（docs/10 §1，2026-10-12 规划书 W1 第 6 项）：
# 在当前已配置架构上启动 QEMU，检查 M1 标记，超时 + 退出码 + 原始日志。
# 用法：bash scripts/run-test.sh "<qemu-bin>" "<qemu-flags>"
# 退出码：0 = 日志检测到 M1 标记；1 = 未检测到或 QEMU 不可用。
set -u
QEMU_BIN="$1"
QEMU_FLAGS="$2"
ARCH_VAL="${ARCH:-unknown}"
LOG="build/test-${ARCH_VAL}.log"

if ! command -v "$QEMU_BIN" >/dev/null 2>&1; then
  echo "FAIL: 未找到 $QEMU_BIN（请安装 QEMU 并加入 PATH，见 docs/07）"
  exit 1
fi

mkdir -p build
# 内核启动后停驻，QEMU 不退出，timeout 结束即视为一次完整启动采样。
timeout 60 "$QEMU_BIN" $QEMU_FLAGS > "$LOG" 2>&1
rc=$?

if grep -q "M1 paging/trap foundation OK" "$LOG"; then
  echo "PASS: $ARCH_VAL 启动至 M1 标记（日志 build/test-$ARCH_VAL.log，qemu rc=$rc）"
  exit 0
fi
echo "FAIL: 日志 build/test-$ARCH_VAL.log 无 M1 标记（qemu rc=$rc）。末尾 20 行："
tail -20 "$LOG"
exit 1