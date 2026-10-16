"""核对 LoongArch UART RX 输入、idle 等待和任务唤醒顺序。"""

import sys
from pathlib import Path


def main() -> int:
    # runner 只会在 idle 标记出现后注入 fixture，成功标记由父任务在 wait4 后输出。
    if len(sys.argv) != 2:
        print("usage: check_la_console.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    if "LA_CONSOLE_FAIL" in serial:
        print("FAIL: parent detected wait status or console write mismatch", file=sys.stderr)
        return 1
    markers = (
        "LA_CONSOLE_WAIT_READY",
        "[console] task 1 waiting for input",
        "[sched] idle waiting for console input",
        "[console] waking task 1",
        "LA_CONSOLE_PASS",
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
        print("FAIL: fatal trap while handling console RX", file=sys.stderr)
        return 1
    print("PASS: LoongArch UART RX woke a blocked console reader")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
