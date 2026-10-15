"""核对 LoongArch 目录、stat 与 dup2 原生用户态验收。"""

import sys
from pathlib import Path


def main() -> int:
    # 标记仅由完成全部目录和 FD 断言的用户程序输出。
    if len(sys.argv) != 2:
        print("usage: check_la_dir.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = ("VFS_BACKENDS_PASS", "LA_DIR_PASS", "LA_USER_EXIT_PASS code=0")
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
    print("PASS: LoongArch mkdirat/getdents64/fstat/dup2")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
