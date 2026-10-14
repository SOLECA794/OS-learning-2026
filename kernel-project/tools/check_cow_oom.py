#!/usr/bin/env python3
"""验证 M2.8 启动自检已遍历 COW fork 的物理帧失败点。"""

import re
import sys
from pathlib import Path


log = Path(sys.argv[1]).read_text()
matches = re.findall(r"M28-FRAME-OOM-PASS points=(\d+)", log)
if (len(matches) != 1 or int(matches[0]) < 2
        or "M28-FORK-OOM-PASS points=9" not in log
        or "M28-COW-WRITE-OOM-PASS" not in log
        or "G6-COW-PASS" not in log):
    raise SystemExit("FAIL: COW OOM 自检未完成或用户态回归失败")
print(f"PASS: COW 地址空间 {matches[0]} 点、完整 fork 早期 9 点及跨页写入 OOM")
