# kernel-project 顶层构建入口
#
# 所有构建必须经本文件：直接调用 cargo 会缺少 build.mk 导出的 RUSTFLAGS（链接脚本）
# 与 feature 集合，产物基址会错（见 AGENTS.md「仓库地图」）。
#
#   make all                              # 一次生成两个架构的 ELF
#   ARCH=riscv64     make defconfig build # 只生成 RISC-V64 ELF
#   ARCH=loongarch64 make defconfig build # 只生成 LoongArch64 ELF

# 配置生成脚本统一通过 Python 3 运行。放在 include build.mk 之前，保证干净仓库执行
# `make all` / `make defconfig` 时也有默认值；命令行仍可用 PYTHON=... 覆盖。
PYTHON ?= python3

# ARCH 的真相源是 config/.config（Kconfig 的 choice），不是命令行变量。
# 命令行传入的 ARCH 只作为「期望值」参与一致性校验，避免两份真相源各说各话
# （docs/07 §2：禁止仅切换 ARCH 而保留另一架构的已解析配置）。
ifneq ($(origin ARCH),undefined)
ARCH_REQUESTED := $(ARCH)
else
ARCH_REQUESTED :=
endif
ARCH ?= riscv64
export ARCH

# 这些目标要在 config/config.mk 不存在（干净仓库）或配置已失效时也能跑，
# 因此不 include build.mk：clean 尤其不能被一份坏配置挡住。
ifeq ($(filter all defconfig menuconfig alldefconfig clean,$(MAKECMDGOALS)),)
include build.mk
endif

# 编排契约（docs/07 §3，P1③）：配置/清理目标与构建目标禁止同条命令。
# `make ARCH=<arch> defconfig clean build` 会跳过 build.mk 导出并删除刚生成的
# 配置，先例见 2026-10-12 规划书 make -n 复现（空 mkdir -p / 空 build / cp /）。
# 命中直接报错，不静默降级。
ifneq ($(filter build check run gdb test,$(MAKECMDGOALS)),)
ifneq ($(filter defconfig alldefconfig menuconfig clean,$(MAKECMDGOALS)),)
$(error 编排错误：配置/清理目标不能与构建目标同条命令。请分步：make clean; make ARCH=<arch> defconfig; make ARCH=<arch> build)
endif
endif

DEFAULT_PHONY := all defconfig alldefconfig menuconfig check build run gdb test fmt fmt-check clean
.PHONY: $(DEFAULT_PHONY)

all:
	$(MAKE) clean
	$(MAKE) ARCH=riscv64 defconfig
	$(MAKE) ARCH=riscv64 build
	$(MAKE) ARCH=loongarch64 defconfig
	$(MAKE) ARCH=loongarch64 build
	@echo "双架构 ELF 已生成："
	@echo "  build/riscv64/kernel.elf"
	@echo "  build/loongarch64/kernel.elf"

## 配置 ----------------------------------------------------------------------

defconfig:
	$(PYTHON) tools/kconfig.py --defconfig config/defconfig-$(ARCH)

alldefconfig:
	$(PYTHON) tools/kconfig.py --alldefconfig

menuconfig:
	$(PYTHON) tools/kconfig.py --menuconfig

## 编译 ----------------------------------------------------------------------

check:
	$(CARGO) check --target $(RUST_TARGET) --features $(RUST_FEATURES) $(CARGO_PROFILE_FLAG)

build:
	@mkdir -p $(BUILD_DIR)
	$(CARGO) build --target $(RUST_TARGET) --features $(RUST_FEATURES) $(CARGO_PROFILE_FLAG)
	@cp $(CARGO_OUT_DIR)/$(KERNEL_NAME) $(KERNEL_ELF)
	@$(PYTHON) tools/check-image.py $(KERNEL_ELF) $(LINK_BASE) $(ARCH)
	@echo "产物：$(KERNEL_ELF)（链接基址 $(LINK_BASE)，入口 _start）"

## 运行（注意：会真正启动 QEMU，仅在用户明确要求时使用）------------------------

run: build
	@command -v $(QEMU_BIN) >/dev/null 2>&1 || \
		{ echo "未找到 $(QEMU_BIN)。请安装 QEMU（>=8.0）并加入 PATH 后再运行 make run。"; exit 1; }
	$(QEMU_BIN) $(QEMU_FLAGS)

gdb: build
	@command -v $(QEMU_BIN) >/dev/null 2>&1 || \
		{ echo "未找到 $(QEMU_BIN)。请安装 QEMU（>=8.0）并加入 PATH 后再运行 make gdb。"; exit 1; }
	$(QEMU_BIN) $(QEMU_FLAGS) -S -s

## 测试 ----------------------------------------------------------------------
# 最小运行测试入口（docs/10 §1，2026-10-12 规划书 W1 第 6 项）：
# 在当前已配置架构上启动 QEMU，检查 M1 启动标记，带超时、退出码与原始日志
# （日志 build/test-<arch>.log）。依赖 QEMU 与 bash（本项目开发环境 = WSL，见 docs/07）。
test: build
	@bash scripts/run-test.sh "$(QEMU_BIN)" "$(QEMU_FLAGS)"

## 格式与清理 ----------------------------------------------------------------

fmt:
	$(CARGO) fmt

fmt-check:
	$(CARGO) fmt --check

clean:
	@rm -rf build target
	@rm -f config/.config config/config.mk
	@echo "已清理 build/ target/ 与生成配置"
