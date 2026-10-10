#!/usr/bin/env python3
"""校验内核映像的 ELF 头是否符合目标平台的装载约定。

为什么需要它：链接基址错了的内核在 QEMU 里通常表现为"没有任何输出"，
与"串口驱动写错"难以区分。这个脚本把「入口地址正确」变成一条可机械判定的门禁，
而不是靠人眼看反汇编（docs/07 §6 验收标准）。

用法：python tools/check-image.py build/riscv64/kernel.elf 0x80200000 riscv64
"""

from __future__ import annotations

import struct
import sys
from pathlib import Path

# ELF e_machine 取值
EM_RISCV = 243
EM_LOONGARCH = 258

MACHINE_NAMES = {EM_RISCV: "riscv64", EM_LOONGARCH: "loongarch64"}


class ImageError(Exception):
    pass


def parse_elf_header(path: Path) -> tuple[int, int, int]:
    """返回 (e_machine, e_entry, is_64bit_le)。"""
    data = path.read_bytes()[:64]
    if data[:4] != b"\x7fELF":
        raise ImageError(f"{path} 不是 ELF 文件")
    ei_class, ei_data = data[4], data[5]
    if ei_class != 2:
        raise ImageError(f"{path} 不是 64 位 ELF（EI_CLASS={ei_class}）")
    if ei_data != 1:
        raise ImageError(f"{path} 不是小端 ELF（EI_DATA={ei_data}）")
    e_machine = struct.unpack_from("<H", data, 18)[0]
    e_entry = struct.unpack_from("<Q", data, 24)[0]
    return e_machine, e_entry, 0


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        print(__doc__.strip())
        return 2
    image, base_str, want_arch = Path(argv[1]), argv[2], argv[3]
    if not image.exists():
        raise ImageError(f"找不到映像 {image}；先跑 make build")

    expected_base = int(base_str, 0)
    e_machine, e_entry, _ = parse_elf_header(image)

    got_arch = MACHINE_NAMES.get(e_machine, f"unknown({e_machine})")
    if got_arch != want_arch:
        raise ImageError(f"架构不匹配：映像为 {got_arch}，期望 {want_arch}")

    if e_entry != expected_base:
        raise ImageError(
            f"入口地址不匹配：e_entry={e_entry:#x}，期望 {expected_base:#x}。"
            "检查 linker/<arch>.ld 的 BASE_ADDRESS 与 build.mk 的 LINK_BASE 是否同步，"
            "以及 .text.entry 是否排在 .text 最前面。"
        )

    print(f"OK  {image}  machine={got_arch}  e_entry={e_entry:#x}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except ImageError as exc:
        print(f"check-image: {exc}", file=sys.stderr)
        sys.exit(1)
