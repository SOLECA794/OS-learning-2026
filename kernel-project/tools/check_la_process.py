"""核对 LoongArch 进程验收日志中的父子状态与 COW 证据。"""

import sys
from pathlib import Path


def main() -> int:
    # run_qemu.py 已确认父进程 exit(0)；这里补充检查中间状态与顺序。
    if len(sys.argv) != 2:
        print("usage: check_la_process.py SERIAL_LOG", file=sys.stderr)
        return 2
    serial = Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace")
    markers = (
        "[fork] task 0 forked task 1",
        "[sched] task 0 -> task 1",
        "[exit] task 1 exited with code 7",
        "[sched] task 1 -> task 0",
        "[wait] reaped task 1 code 7",
        "LA_PROCESS_PASS",
        "[exit] task 0 exited with code 0",
        "LA_USER_EXIT_PASS code=0",
    )
    position = 0
    for marker in markers:
        found = serial.find(marker, position)
        if found < 0:
            print(f"FAIL: missing or out-of-order marker: {marker}", file=sys.stderr)
            return 1
        position = found + len(marker)
    # 子任务用户写入和父任务 wait4 写回分别触发一页 COW 复制。
    if serial.count("[cow] copy vpn=") < 2 or "fatal trap" in serial:
        print("FAIL: COW copies missing or fatal trap present", file=sys.stderr)
        return 1
    print("PASS: LoongArch fork/COW/exit/wait4 sequence")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
