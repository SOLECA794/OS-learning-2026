"""核对 LoongArch COW 页帧故障注入与用户态 fork/COW 回归日志。"""

import re
import sys
from pathlib import Path


def main() -> int:
    # 必须同时完成全分配点注入和真实 PLV3 父子 COW 场景。
    if len(sys.argv) != 2:
        print("usage: check_la_cow_oom.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    match = re.search(r"M28-FRAME-OOM-PASS points=(\d+)", serial)
    if not match or int(match.group(1)) <= 0:
        print("FAIL: missing frame-allocation OOM sweep", file=sys.stderr)
        return 1
    if "M28-COW-WRITE-OOM-PASS" not in serial:
        print("FAIL: missing recoverable COW write OOM check", file=sys.stderr)
        return 1
    if "M28-COW-RECLAIM-PASS rounds=128" not in serial:
        print("FAIL: missing 128-round COW resource reclamation sweep", file=sys.stderr)
        return 1
    if "M28-FORK-OOM-PASS points=9" not in serial:
        print("FAIL: missing early task-fork OOM checks", file=sys.stderr)
        return 1
    forks = re.findall(r"\[fork\] task 0 forked task (\d+)", serial)
    reaps = re.findall(r"\[wait\] reaped task (\d+) code 7", serial)
    if len(forks) != 128 or len(reaps) != 128 or forks[-1] != reaps[-1]:
        print("FAIL: expected 128 user fork/wait4 cycles", file=sys.stderr)
        return 1
    if "LA_COW_STRESS_PASS" not in serial:
        print("FAIL: missing user COW stress success marker", file=sys.stderr)
        return 1
    if "fatal trap" in serial or "LA_USER_EXIT_PASS code=0" not in serial:
        print("FAIL: LoongArch user process did not exit cleanly", file=sys.stderr)
        return 1
    print(
        f"PASS: LoongArch M2.8 ({match.group(1)} injected points, "
        f"128 user fork/reap cycles)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
