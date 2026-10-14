#!/usr/bin/env python3
"""校验 uapp-cow 的重复回收：功能标记与物理帧计数必须同时通过。"""

import re
import sys
from pathlib import Path


def main():
    """日志来自有界 QEMU runner；计数是诊断证据，不参与内核行为决策。"""
    log = Path(sys.argv[1]).read_text()
    counts = [int(n) for n in re.findall(
        r"\[wait\] reaped task \d+ code 0  \(frames=(\d+) wm=\d+\)", log
    )]
    # uapp-cow 只有重复回收子任务使用退出码 0；多代用例使用 7 和 9。
    if "G6-COW-PASS" not in log or len(counts) != 128 or len(set(counts)) != 1:
        raise SystemExit(f"FAIL: COW 回收计数不稳定或用例未完成: {counts}")
    # 排除启动自检的 COW 日志，只统计用户任务运行后的复制量。
    user_log = log.split("M3: task 0 ready", 1)[-1]
    forks = len(re.findall(r"\[fork\] task \d+ forked task \d+", user_log))
    copies = len(re.findall(r"\[cow\] copy vpn=", user_log))
    if forks < 130 or not (0 < copies < 3 * forks):
        raise SystemExit(f"FAIL: COW 页复制量异常: forks={forks} copies={copies}")
    print(f"PASS: COW 连续 128 轮回收后 frames={counts[0]}；forks={forks} copies={copies}")


if __name__ == "__main__":
    main()
