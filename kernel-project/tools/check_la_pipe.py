"""核对 LoongArch pipe 用户态读写、重定向与最终退出。"""

import sys
from pathlib import Path


def main() -> int:
    # 原生用户程序在所有 FD 与内容断言通过后才打印标记。
    if len(sys.argv) != 2:
        print("usage: check_la_pipe.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = ("VFS_BACKENDS_PASS", "LA_PIPE_PASS", "LA_USER_EXIT_PASS code=0")
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
    print("PASS: LoongArch pipe, dup2 stdout redirection and EFAULT cleanup")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
