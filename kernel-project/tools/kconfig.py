#!/usr/bin/env python3
"""极简 Kconfig 解析器（本仓库自研，零第三方依赖）。

为什么自研而不是用 kconfiglib：
- docs/00 §5「最小依赖」与 docs/07 §4「离线可复现」要求构建链不依赖 pip 生态；
- 本仓库只需要 bool / string / int / choice 四种类型与 `depends on`、`default ... if`，
  不需要 tristate、模块、source 递归、环境变量展开，自研规模可控且行为可审计。

产出：
- config/.config        人类可读的当前配置（生成物，不入库）
- config/config.mk      供 build.mk 消费的 `CONFIG_*` 变量（生成物，不入库）

用法：
    python tools/kconfig.py --defconfig config/defconfig-riscv64
    python tools/kconfig.py --set LOG_LEVEL=trace
    python tools/kconfig.py --alldefconfig
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
KCONFIG_FILE = REPO_ROOT / "config" / "Kconfig"
DOT_CONFIG = REPO_ROOT / "config" / ".config"
CONFIG_MK = REPO_ROOT / "config" / "config.mk"

TRUE = "y"
FALSE = "n"


class KconfigError(Exception):
    pass


class Symbol:
    def __init__(self, name: str, stype: str) -> None:
        self.name = name
        self.type = stype  # bool | string | int | choice-member
        self.prompt: str | None = None
        self.defaults: list[tuple[list[str], str]] = []  # (expr_tokens, value_tokens)
        self.depends: list[list[str]] = []  # 多个 depends on 之间取 AND
        self.help: str = ""
        self.choice: "Choice | None" = None

    @property
    def is_bool(self) -> bool:
        return self.type == "bool"


class Choice:
    def __init__(self) -> None:
        self.prompt: str | None = None
        self.members: list[Symbol] = []
        self.default: str | None = None
        self.depends: list[list[str]] = []


class Menu:
    """已解析的 Kconfig 模型。"""

    def __init__(self) -> None:
        self.symbols: dict[str, Symbol] = {}
        self.order: list[str] = []
        self.choices: list[Choice] = []
        self.title = ""

    # ---------- 解析 ----------

    def parse(self, text: str) -> None:
        lines = text.splitlines()
        i = 0
        stack: list = []  # Choice 或 None(menu)
        while i < len(lines):
            raw = lines[i]
            line = raw.strip()
            i += 1
            if not line or line.startswith("#"):
                continue

            head = line.split(None, 1)
            keyword = head[0]
            rest = head[1].strip() if len(head) > 1 else ""

            if keyword == "mainmenu":
                self.title = _unquote(rest)
            elif keyword == "menu":
                stack.append(None)
            elif keyword == "endmenu":
                _pop_menu(stack)
            elif keyword == "choice":
                ch = Choice()
                self.choices.append(ch)
                stack.append(ch)
            elif keyword == "endchoice":
                if not stack or not isinstance(stack[-1], Choice):
                    raise KconfigError("endchoice 没有匹配的 choice")
                stack.pop()
            elif keyword in ("prompt", "default") and stack and isinstance(stack[-1], Choice):
                # choice 块自身的 prompt / default，不属于任何 config
                if keyword == "prompt":
                    stack[-1].prompt = _unquote(rest)
                else:
                    stack[-1].default = rest.split()[0]
            elif keyword == "config":
                sym = Symbol(rest.split()[0], "bool")
                if sym.name in self.symbols:
                    raise KconfigError(f"符号重复定义: {sym.name}")
                self.symbols[sym.name] = sym
                self.order.append(sym.name)
                if stack and isinstance(stack[-1], Choice):
                    sym.choice = stack[-1]
                    stack[-1].members.append(sym)
                i = self._parse_attrs(lines, i, sym, stack)
            elif keyword == "comment":
                continue
            else:
                raise KconfigError(f"无法识别的 Kconfig 关键字: {keyword!r}")

    def _parse_attrs(self, lines: list[str], i: int, sym: Symbol, stack: list) -> int:
        base_indent = None
        in_help = False
        while i < len(lines):
            raw = lines[i]
            if not raw.strip():
                i += 1
                continue
            indent = len(raw) - len(raw.lstrip())
            if base_indent is None:
                base_indent = indent
            if indent <= 0 and not raw.strip().startswith(("bool", "string", "int")):
                break
            line = raw.strip()
            i += 1

            if in_help:
                if indent > base_indent:
                    sym.help += line + "\n"
                    continue
                in_help = False

            head = line.split(None, 1)
            kw = head[0]
            rest = head[1].strip() if len(head) > 1 else ""

            if kw in ("bool", "string", "int"):
                sym.type = kw
                if rest:
                    sym.prompt = _unquote(rest)
            elif kw == "prompt":
                sym.prompt = _unquote(rest)
            elif kw == "default":
                value, cond = _split_if(rest)
                sym.defaults.append((_tokenize(cond) if cond else [], _tokenize(value)))
            elif kw == "depends":
                if not rest.startswith("on"):
                    raise KconfigError(f"{sym.name}: depends 语法错误: {line}")
                sym.depends.append(_tokenize(rest[2:].strip()))
            elif kw == "help":
                in_help = True
                sym.help = ""
            else:
                raise KconfigError(f"{sym.name}: 无法识别的属性: {kw!r}")

        # menu/choice 的缩进层级由 _pop_menu 处理，此处不吞掉下一行的 indent
        return i

    # ---------- 求值 ----------

    def eval_expr(self, tokens: list[str], values: dict[str, str]) -> bool:
        if not tokens:
            return True
        return _Evaluator(self, values).parse(tokens)

    def symbol_value(self, name: str, values: dict[str, str]) -> str:
        sym = self.symbols.get(name)
        if sym is None:
            raise KconfigError(f"表达式引用了未定义的符号: {name}")
        return values.get(name, FALSE if sym.is_bool else "")

    def visible(self, sym: Symbol, values: dict[str, str]) -> bool:
        for dep in sym.depends:
            if not self.eval_expr(dep, values):
                return False
        if sym.choice is not None:
            for dep in sym.choice.depends:
                if not self.eval_expr(dep, values):
                    return False
        return True

    def resolve(self, user: dict[str, str]) -> dict[str, str]:
        """按默认值填充用户未指定的符号；返回完整配置。

        采用两趟：先算非 choice 成员，再定 choice 归属（choice 必须恰好选一个）。
        """
        values: dict[str, str] = {}

        for name in self.order:
            sym = self.symbols[name]
            if sym.choice is not None:
                continue
            values[name] = self._resolve_symbol(sym, user, values)

        for ch in self.choices:
            if not ch.members:
                continue
            for m in ch.members:
                values[m.name] = FALSE
            selected = None
            # 1) 用户在 .config 里显式选中的
            for m in ch.members:
                if user.get(m.name) in (TRUE, "y"):
                    selected = m.name
                    break
            # 2) choice 的 default
            if selected is None and ch.default:
                cand = self.symbols.get(ch.default)
                if cand is not None and cand in ch.members and self.visible(cand, values):
                    selected = cand.name
            # 3) 第一个可见成员
            if selected is None:
                for m in ch.members:
                    if self.visible(m, values):
                        selected = m.name
                        break
            if selected is None:
                raise KconfigError(f"choice({ch.prompt}) 没有可选成员")
            values[selected] = TRUE

        return values

    def _resolve_symbol(self, sym: Symbol, user: dict[str, str], values: dict[str, str]) -> str:
        if not self.visible(sym, values):
            return FALSE if sym.is_bool else ""
        if sym.name in user:
            return _coerce(sym, user[sym.name])
        for cond, value_tokens in sym.defaults:
            if self.eval_expr(cond, values):
                val = self._eval_value(value_tokens, values)
                return _coerce(sym, val)
        return FALSE if sym.is_bool else ""

    def _eval_value(self, tokens: list[str], values: dict[str, str]) -> str:
        if len(tokens) == 1:
            tok = tokens[0]
            if tok in self.symbols:
                return self.symbol_value(tok, values)
            return tok.strip('"')
        return " ".join(t.strip('"') for t in tokens)


class _Evaluator:
    """布尔表达式递归下降求值：支持 ! && || == != 括号 与符号引用。"""

    def __init__(self, menu: Menu, values: dict[str, str]) -> None:
        self.menu = menu
        self.values = values
        self.toks: list[str] = []
        self.pos = 0

    def parse(self, tokens: list[str]) -> bool:
        self.toks = tokens
        self.pos = 0
        return self._or()

    def _peek(self) -> str | None:
        return self.toks[self.pos] if self.pos < len(self.toks) else None

    def _next(self) -> str:
        tok = self.toks[self.pos]
        self.pos += 1
        return tok

    def _or(self) -> bool:
        left = self._and()
        while self._peek() == "||":
            self._next()
            right = self._and()
            left = left or right
        return left

    def _and(self) -> bool:
        left = self._unary()
        while self._peek() == "&&":
            self._next()
            right = self._unary()
            left = left and right
        return left

    def _unary(self) -> bool:
        if self._peek() == "!":
            self._next()
            return not self._unary()
        return self._cmp()

    def _cmp(self) -> bool:
        left = self._atom()
        op = self._peek()
        if op in ("=", "==", "!="):
            self._next()
            right = self._atom()
            eq = left == right
            return eq if op != "!=" else not eq
        return left

    def _atom(self) -> bool | str:
        tok = self._next()
        if tok == "(":
            val = self._or()
            if self._peek() == ")":
                self._next()
            return val
        if tok in self.symbols:
            v = self.menu.symbol_value(tok, self.values)
            return v not in (FALSE, "n", "")
        return tok.strip('"') not in (FALSE, "n", "")


def _pop_menu(stack: list) -> None:
    if not stack:
        raise KconfigError("endmenu 没有匹配的 menu")
    if isinstance(stack[-1], Choice):
        raise KconfigError("endmenu 与 choice 不匹配")
    stack.pop()


def _unquote(s: str) -> str:
    s = s.strip()
    if len(s) >= 2 and s[0] == s[-1] and s[0] in "\"'":
        return s[1:-1]
    return s


def _split_if(s: str) -> tuple[str, str | None]:
    parts = s.split(" if ", 1)
    if len(parts) == 1:
        return s.strip(), None
    return parts[0].strip(), parts[1].strip()


def _tokenize(s: str) -> list[str]:
    return [t for t in s.replace("(", " ( ").replace(")", " ) ").split() if t]


def _coerce(sym: Symbol, value: str) -> str:
    if sym.type == "bool":
        return TRUE if value in ("y", "Y", "1", "true") else FALSE
    if sym.type == "int":
        try:
            int(value, 0)
        except ValueError as exc:
            raise KconfigError(f"{sym.name}: 不是合法的 int: {value!r}") from exc
        return value
    return value.strip('"')


def read_config(path: Path) -> dict[str, str]:
    """读取 .config / defconfig：`CONFIG_X=y`、`# CONFIG_X is not set`、`CONFIG_X=...`。"""
    values: dict[str, str] = {}
    if not path.exists():
        return values
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") and "is not set" not in line:
            continue
        # 符号名一律去掉 CONFIG_ 前缀存储：menu.symbols 用的是 Kconfig 里的裸符号名。
        if line.startswith("#") and line.endswith("is not set"):
            values[line[2:].split()[0].removeprefix("CONFIG_")] = FALSE
            continue
        if not line.startswith("CONFIG_"):
            continue
        name, _, raw = line.partition("=")
        values[name.removeprefix("CONFIG_")] = raw.strip().strip('"')
    return values


def write_outputs(menu: Menu, values: dict[str, str]) -> None:
    header = (
        "# 自动生成，请勿手工编辑。\n"
        f"# 源：config/Kconfig（由 tools/kconfig.py 生成）\n"
    )
    dot = [header]
    mk = [
        "# 自动生成，请勿手工编辑；由 tools/kconfig.py 从 config/Kconfig 生成。\n"
        "# 本文件是 build.mk 的唯一输入：bool/choice 由其映射为 Cargo feature，\n"
        "# int/string 是直接供脚本使用的运行时配置（docs/07 §2：QEMU 启动值走脚本而非 feature）。\n"
    ]
    for name in menu.order:
        sym = menu.symbols[name]
        val = values.get(name, FALSE if sym.is_bool else "")
        if sym.is_bool:
            dot.append(f"CONFIG_{name}={val}\n" if val == TRUE else f"# CONFIG_{name} is not set\n")
        else:
            dot.append(f'CONFIG_{name}="{val}"\n' if sym.type == "string" else f"CONFIG_{name}={val}\n")

        if sym.is_bool:
            mk.append(f"CONFIG_{name} := {val}\n")
        elif sym.type == "string":
            mk.append(f"CONFIG_{name} := {val}\n")
        else:
            mk.append(f"CONFIG_{name} := {val}\n")
    DOT_CONFIG.write_text("".join(dot), encoding="utf-8")
    CONFIG_MK.write_text("".join(mk), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="极简 Kconfig 解析器")
    ap.add_argument("--defconfig", type=Path, help="从 defconfig 文件起步")
    ap.add_argument("--alldefconfig", action="store_true", help="全部取默认值，忽略已有 .config")
    ap.add_argument("--set", action="append", default=[], metavar="SYM=VAL",
                    help="覆盖某个符号（可重复），SYM 不带 CONFIG_ 前缀")
    ap.add_argument("--menuconfig", action="store_true",
                    help="交互配置；需要 kconfiglib，未安装时报错并提示改用 --defconfig/--set")
    ap.add_argument("--print", action="store_true", help="把最终配置打到 stdout")
    args = ap.parse_args(argv)

    if args.menuconfig:
        print(
            "menuconfig 需要 kconfiglib（pip install kconfiglib，交互界面在 Windows 还需 windows-curses）。\n"
            "当前仓库走自研解析器，请改用：\n"
            "  python tools/kconfig.py --defconfig config/defconfig-<arch>\n"
            "  python tools/kconfig.py --set LOG_LEVEL=trace\n",
            file=sys.stderr,
        )
        return 2

    menu = Menu()
    menu.parse(KCONFIG_FILE.read_text(encoding="utf-8"))

    user: dict[str, str] = {}
    if args.alldefconfig:
        pass
    elif args.defconfig:
        # defconfig 的语义是"重置为默认"，刻意**不**与已有 .config 合并：
        # 否则切架构时旧的 ARCH_*=y 会残留，choice 会选回上一个架构
        # （docs/07 §2：禁止仅切换 ARCH 而保留另一架构的已解析配置）。
        user.update(read_config(args.defconfig))
    else:
        user.update(read_config(DOT_CONFIG))

    for item in args.set:
        name, _, val = item.partition("=")
        name = name.removeprefix("CONFIG_")
        if name not in menu.symbols:
            raise KconfigError(f"未知符号: {name}")
        sym = menu.symbols[name]
        if sym.choice is not None:
            # choice 是"恰好选一个"：选中一个成员必须把同组其它成员清掉，
            # 否则 resolve() 会按声明顺序挑到先出现的那个，--set 看着像没生效。
            for m in sym.choice.members:
                user[m.name] = FALSE
        user[name] = val

    values = menu.resolve(user)
    write_outputs(menu, values)

    if args.print:
        for name in menu.order:
            print(f"CONFIG_{name}={values.get(name)}")
    else:
        print(f"已生成 {DOT_CONFIG.relative_to(REPO_ROOT)} 与 {CONFIG_MK.relative_to(REPO_ROOT)}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except KconfigError as exc:
        print(f"kconfig: {exc}", file=sys.stderr)
        sys.exit(1)
