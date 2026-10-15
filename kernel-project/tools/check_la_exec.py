"""检查 LoongArch exec 的新程序标记与退出顺序。"""

import sys
from pathlib import Path


def main() -> int:
    # QEMU runner 已检查退出码；这里要求目标程序确实执行。
    if len(sys.argv) != 2:
        print("usage: check_la_exec.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = ("LA_EXEC_PASS", "[exit] task 0 exited with code 0", "LA_USER_EXIT_PASS code=0")
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
    print("PASS: LoongArch exec failure rollback and successful replacement")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
