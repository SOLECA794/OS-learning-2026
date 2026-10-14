#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""有界 QEMU 运行器（V2 总路线 §11）。

由 Make 调用，统一负责自动测试：校验输入与冻结 ELF → 创建独立日志目录 →
启动独立进程组 → 持续采集输出并检查截止时间 → 判定结果 → 回收进程 → 写 result.json。

合同要点（V2 §11）：
- 构建和运行各有预算；本脚本只负责运行预算（boot <=20s / selftest <=60s），
  失败后不得自动逐次加长截止时间。
- 日志目录 artifacts/runs/<run-id>/，加入忽略列表且 make clean 不删除；
  每次新建 serial.log / qemu.stderr.log / result.json，禁止追加旧日志。
- 有界拼行并保留原始字节；stdout/stderr 都不能阻塞 Guest；start_new_session
  建立独立进程组（不使用不安全的 preexec_fn）。
- KTEST 事件（BEGIN/CASE/END）以换行结束，由实际断言产生；用户程序普通输出
  不能充当内核断言结果；未执行的用例不准打印 PASS。
- make 对外只承诺 0/非 0，详细原因读 result.json。
- 可选输入 fixture 在串口输出命中指定标记后通过 PTY 注入；默认仍关闭 stdin。

用法：
  python tools/run_qemu.py --qemu <bin> --flags "<flags>" \
      --arch <riscv64|loongarch64> [--mode boot|selftest] \
      [--timeout <s>] [--pattern <regex>] [--run-id <id>]

退出码：0 = 测试通过；1 = 测试失败或运行器错误。
"""
import argparse
import errno
import hashlib
import json
import os
import pty
import re
import select
import signal
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RUNS_DIR = os.path.join(ROOT, "artifacts", "runs")
MAX_LINE = 8192
GRACE = 3
POLL = 0.05

DEFAULT_PATTERNS = {
    "boot": r"M1 paging/trap foundation OK",
    "selftest": r"KTEST END",
}


def git_rev():
    try:
        out = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=ROOT, capture_output=True, text=True, timeout=5,
        )
        if out.returncode == 0:
            return out.stdout.strip()
    except Exception:
        pass
    return "unknown"


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def qemu_version(bin_path):
    try:
        out = subprocess.run(
            [bin_path, "--version"], capture_output=True, text=True, timeout=5,
        )
        return out.stdout.strip().splitlines()[0] if out.returncode == 0 else "unknown"
    except Exception:
        return "unknown"


def which(name):
    import shutil
    return shutil.which(name)


class LineAssembler:
    """有界拼行：保留原始字节，超长强制断行（洪泛防护），不阻塞调用方。"""

    def __init__(self, sink, max_len=MAX_LINE):
        self.sink = sink
        self.max_len = max_len
        self.pending = bytearray()

    def feed(self, chunk: bytes):
        while chunk:
            nl = chunk.find(b"\n")
            if nl < 0:
                self.pending += chunk
                if len(self.pending) > self.max_len:
                    self.sink(bytes(self.pending))
                    self.pending.clear()
                return
            self.pending += chunk[:nl]
            line = bytes(self.pending)
            self.pending.clear()
            self.sink(line)
            chunk = chunk[nl + 1:]

    def finish(self):
        if self.pending:
            self.sink(bytes(self.pending))
            self.pending.clear()


class KtestParser:
    """KTEST 事件登记：BEGIN/CASE/END。未登记结束或未执行的用例不算 PASS。"""

    BEGIN = re.compile(rb"KTEST BEGIN suite=(\S+)")
    CASE = re.compile(rb"KTEST CASE name=(\S+) result=(PASS|FAIL)")
    END = re.compile(rb"KTEST END suite=(\S+) passed=(\d+) failed=(\d+)")

    def __init__(self):
        self.suite = None
        self.cases = []
        self.end_seen = None

    def feed(self, line: bytes):
        m = self.BEGIN.search(line)
        if m and self.suite is None:
            self.suite = m.group(1).decode("utf-8", "replace")
            self.cases = []
            return
        m = self.END.search(line)
        if m:
            self.end_seen = {
                "suite": m.group(1).decode("utf-8", "replace"),
                "passed": int(m.group(2)),
                "failed": int(m.group(3)),
            }
            return
        m = self.CASE.search(line)
        if m:
            self.cases.append({
                "name": m.group(1).decode("utf-8", "replace"),
                "result": m.group(2).decode("utf-8", "replace"),
            })

    def summary(self):
        if self.end_seen is None:
            return None
        return {
            "suite": self.end_seen["suite"],
            "passed": self.end_seen["passed"],
            "failed": self.end_seen["failed"],
            "cases": self.cases,
        }


def main():
    ap = argparse.ArgumentParser(description="有界 QEMU 运行器（V2 §11）")
    ap.add_argument("--qemu", required=True)
    ap.add_argument("--flags", required=True)
    ap.add_argument("--arch", default="riscv64")
    ap.add_argument("--mode", choices=["boot", "selftest"], default="boot")
    ap.add_argument("--timeout", type=int, default=None)
    ap.add_argument("--pattern", default=None)
    ap.add_argument("--run-id", default=None)
    ap.add_argument("--elf", default=None)
    ap.add_argument("--stdin-file", default=None,
                    help="最多 4096 字节的串口输入 fixture")
    ap.add_argument("--stdin-after", default=None,
                    help="串口输出命中此正则后才注入 fixture")
    args = ap.parse_args()

    if bool(args.stdin_file) != bool(args.stdin_after):
        ap.error("--stdin-file 和 --stdin-after 必须同时指定")
    fixture = None
    fixture_sha = None
    if args.stdin_file:
        with open(args.stdin_file, "rb") as f:
            fixture = f.read(4097)
        if not fixture or len(fixture) > 4096:
            ap.error("输入 fixture 必须为 1..4096 字节")
        if b"\x01" in fixture:
            ap.error("输入 fixture 不得含 Ctrl-A（QEMU mon:stdio 转义键）")
        fixture_sha = hashlib.sha256(fixture).hexdigest()
    input_marker = re.compile(args.stdin_after.encode("utf-8")) if fixture else None

    qemu = args.qemu
    flags = args.flags
    arch = args.arch
    mode = args.mode
    pattern = args.pattern or DEFAULT_PATTERNS[mode]
    pattern_re = re.compile(pattern.encode("utf-8"))
    timeout = args.timeout or (20 if mode == "boot" else 60)

    if not os.path.isfile(qemu) and which(qemu) is None:
        fail_fast("工具缺失", "未找到 QEMU 可执行文件: %s" % qemu, args, arch)
        return 1

    elf_path = args.elf or os.path.join(ROOT, "build", arch, "kernel.elf")
    if not os.path.isfile(elf_path):
        fail_fast("构建产物缺失", "未找到内核 ELF: %s（先 make build）" % elf_path, args, arch)
        return 1
    elf_sha = sha256_file(elf_path)

    run_id = args.run_id or ("%s-%s" % (mode, time.strftime("%Y%m%d-%H%M%S")))
    run_dir = os.path.join(RUNS_DIR, run_id)
    os.makedirs(run_dir, exist_ok=True)
    serial_path = os.path.join(run_dir, "serial.log")
    stderr_path = os.path.join(run_dir, "qemu.stderr.log")
    result_path = os.path.join(run_dir, "result.json")
    for p in (serial_path, stderr_path, result_path):
        if os.path.exists(p):
            os.remove(p)

    start = time.time()
    deadline = start + timeout

    serial_master = None
    serial_slave = None
    qemu_argv = [qemu] + flags.split()
    try:
        if fixture:
            # `mon:stdio` 在普通 stdin pipe 上直到 EOF 才交付输入；EOF 又可能
            # 直接让 QEMU 退出。PTY 与真实交互终端一致，既能即时交付也保持连接。
            serial_master, serial_slave = pty.openpty()
        proc = subprocess.Popen(
            qemu_argv,
            stdout=serial_slave if fixture else subprocess.PIPE,
            stderr=subprocess.PIPE,
            stdin=serial_slave if fixture else subprocess.DEVNULL,
            start_new_session=True,
            bufsize=0,
        )
    except FileNotFoundError:
        if serial_master is not None:
            os.close(serial_master)
            os.close(serial_slave)
        fail_fast("工具缺失", "无法启动 QEMU: %s" % qemu, args, arch)
        return 1
    if serial_slave is not None:
        os.close(serial_slave)

    ktest = KtestParser()
    matched = {"hit": False, "when": None}
    input_state = {"offset": 0, "armed": False, "sent": False}
    # 滚动窗口：跨 chunk / 无换行 pattern 检测（fatal trap 等尾行常无 \n）。
    scan_tail = bytearray()
    tail_win = MAX_LINE + len(pattern_re.pattern) + (len(input_marker.pattern) if input_marker else 0) + 4096

    def on_line(line: bytes):
        ktest.feed(line)
        if not matched["hit"] and pattern_re.search(line):
            matched["hit"] = True
            matched["when"] = round(time.time() - start, 3)

    serial_asm = LineAssembler(on_line)

    stdout_fd = serial_master if fixture else proc.stdout.fileno()
    stderr_fd = proc.stderr.fileno()
    eof = {stdout_fd: False, stderr_fd: False}
    reason = None
    while True:
        now = time.time()
        if now >= deadline:
            reason = "EXIT_TIMEOUT"
            break
        try:
            rlist, _, _ = select.select([stdout_fd, stderr_fd], [], [], POLL)
        except (InterruptedError, OSError):
            continue
        for fd in rlist:
            if fd == stdout_fd and not eof[fd]:
                try:
                    buf = os.read(fd, 65536)
                except OSError as exc:
                    # Linux PTY master 在 slave 关闭后以 EIO 报告 EOF。
                    if fixture and exc.errno == errno.EIO:
                        buf = b""
                    else:
                        raise
                if buf:
                    with open(serial_path, "ab") as lf:
                        lf.write(buf)
                    serial_asm.feed(buf)
                    # 滚动窗口检测：覆盖跨 chunk、无换行、行尾不完整的 pattern。
                    scan_tail += buf
                    if len(scan_tail) > tail_win:
                        del scan_tail[: len(scan_tail) - tail_win]
                    if not matched["hit"] and pattern_re.search(bytes(scan_tail)):
                        matched["hit"] = True
                        matched["when"] = round(time.time() - start, 3)
                    if input_marker and not input_state["armed"] and input_marker.search(bytes(scan_tail)):
                        input_state["armed"] = True
                else:
                    eof[fd] = True
            elif fd == stderr_fd and not eof[fd]:
                buf = os.read(fd, 65536)
                if buf:
                    with open(stderr_path, "ab") as lf:
                        lf.write(buf)
                else:
                    eof[fd] = True
        if fixture and input_state["armed"] and not input_state["sent"]:
            # 小 fixture 分段非阻塞写；Guest 不读取时仍受总截止时间保护。
            _, writable, _ = select.select([], [serial_master], [], 0)
            if writable:
                offset = input_state["offset"]
                try:
                    written = os.write(serial_master, fixture[offset:])
                except BrokenPipeError:
                    reason = "INPUT_PIPE_CLOSED"
                    break
                input_state["offset"] += written
                if input_state["offset"] == len(fixture):
                    input_state["sent"] = True
                    # PTY 保持打开，直到本次有界运行完成。
        if matched["hit"]:
            if proc.poll() is not None:
                reason = "NORMAL_EXIT"
            else:
                reason = "MATCHED"
            break

    if proc.poll() is None:
        try:
            proc.terminate()
        except ProcessLookupError:
            pass
        try:
            proc.wait(timeout=GRACE)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            try:
                proc.wait(timeout=GRACE)
            except subprocess.TimeoutExpired:
                pass
    try:
        os.killpg(proc.pid, signal.SIGKILL)
        proc.wait(timeout=1)
    except (ProcessLookupError, subprocess.TimeoutExpired, PermissionError):
        pass

    end = time.time()
    raw_rc = proc.returncode
    serial_asm.finish()
    if serial_master is not None:
        os.close(serial_master)

    ktest_summary = ktest.summary()
    passed = False
    if reason in ("MATCHED", "NORMAL_EXIT"):
        if mode == "selftest":
            passed = ktest_summary is not None and ktest_summary["failed"] == 0
            if not passed:
                reason = "TEST_FAILED"
        else:
            passed = True
    elif reason == "EXIT_TIMEOUT":
        passed = False
    else:
        passed = False
        reason = reason or "UNKNOWN"
    if fixture and not input_state["sent"]:
        passed = False
        reason = "INPUT_NOT_SENT" if reason in ("MATCHED", "NORMAL_EXIT") else reason

    result = {
        "run_id": run_id,
        "mode": mode,
        "arch": arch,
        "version": git_rev(),
        "source": {"repo": "OS-learning-2026", "commit": git_rev()},
        "elf": {"path": elf_path, "sha256": elf_sha},
        "qemu": {"version": qemu_version(qemu), "argv": qemu_argv},
        "time": {"start": start, "end": end, "elapsed_s": round(end - start, 3),
                 "budget_s": timeout},
        "exit": {"code": raw_rc, "reason": reason},
        "logs": {"serial": serial_path, "stderr": stderr_path},
        "pattern": pattern,
        "stdin": {"fixture_sha256": fixture_sha, "after": args.stdin_after,
                  "sent": input_state["sent"]} if fixture else None,
        "ktest": ktest_summary,
        "passed": passed,
    }
    with open(result_path, "w", encoding="utf-8") as f:
        json.dump(result, f, ensure_ascii=False, indent=2)

    if passed:
        print("PASS: %s %s（run-id=%s，reason=%s，elf sha256=%s，qemu rc=%s）"
              % (arch, mode, run_id, reason, elf_sha[:12], raw_rc))
        return 0
    print("FAIL: %s %s（run-id=%s，reason=%s，qemu rc=%s）" % (arch, mode, run_id, reason, raw_rc))
    print("  日志：%s" % serial_path)
    with open(serial_path, "rb") as f:
        tail = f.read()[-2048:].decode("utf-8", "replace")
    print("  serial 末尾：")
    for ln in tail.splitlines()[-20:]:
        print("   |", ln)
    return 1


def fail_fast(kind, msg, args, arch):
    run_id = args.run_id or ("%s-%s" % (args.mode, time.strftime("%Y%m%d-%H%M%S")))
    run_dir = os.path.join(RUNS_DIR, run_id)
    os.makedirs(run_dir, exist_ok=True)
    result_path = os.path.join(run_dir, "result.json")
    payload = {
        "run_id": run_id,
        "mode": args.mode,
        "arch": arch,
        "version": git_rev(),
        "source": {"repo": "OS-learning-2026", "commit": git_rev()},
        "error": {"kind": kind, "message": msg},
        "passed": False,
    }
    with open(result_path, "w", encoding="utf-8") as f:
        json.dump(payload, f, ensure_ascii=False, indent=2)
    print("FAIL: %s：%s（result.json 已写 %s）" % (kind, msg, result_path))


if __name__ == "__main__":
    sys.exit(main())
