"""核对 LoongArch 文件 syscall 的用户态通过标记和最终退出。"""

import sys
from pathlib import Path


def main() -> int:
    # 测试程序只在所有文件内容、errno、EOF 与关闭检查通过后打印标记。
    if len(sys.argv) != 2:
        print("usage: check_la_file.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = ("VFS_BACKENDS_PASS", "LA_FILE_PASS", "LA_USER_EXIT_PASS code=0")
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
    print("PASS: LoongArch ramfs/staticfs openat/read/close and errors")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
