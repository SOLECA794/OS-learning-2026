"""确认父进程忙等时子进程仍通过时钟抢占获得运行机会。"""

import sys
from pathlib import Path


def main() -> int:
    # 不能仅看 exit 标记：必须先看到 fork 和调度到子进程，再见用户标记。
    if len(sys.argv) != 2:
        print("usage: check_la_preempt.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = (
        "[fork] task 0 forked task 1",
        "[sched] task 0 -> task 1",
        "LA_PREEMPT_PASS",
        "[exit] task 1 exited with code 0",
        "LA_USER_EXIT_PASS code=0",
    )
    position = 0
    for marker in markers:
        found = serial.find(marker, position)
        if found < 0:
            print(f"FAIL: missing or out-of-order marker: {marker}", file=sys.stderr)
            return 1
        position = found + len(marker)
    if "fatal trap" in serial:
        print("FAIL: fatal trap present", file=sys.stderr)
        return 1
    print("PASS: LoongArch timer preempted busy parent and ran child")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
