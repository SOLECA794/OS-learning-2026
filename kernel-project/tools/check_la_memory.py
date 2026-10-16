"""核对 LoongArch brk、匿名 mmap 与 munmap 用户态验收。"""

import sys
from pathlib import Path


def main() -> int:
    # 用户程序只在堆和匿名页断言通过后打印标记。
    if len(sys.argv) != 2:
        print("usage: check_la_memory.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = ("VFS_BACKENDS_PASS", "LA_MEMORY_PASS", "LA_USER_EXIT_PASS code=0")
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
    print("PASS: LoongArch brk/mmap/munmap user mappings")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
