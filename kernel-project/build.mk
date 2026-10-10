# build.mk —— 把 Kconfig 配置转译为 Cargo/链接/QEMU 参数。
#
# 链路（docs/07 §2）：
#   config/Kconfig → config/config.mk（由 tools/kconfig.py 生成）
#                  → 本文件（架构派生值 + RUST_FEATURES + 脚本变量）
#                  → Cargo feature / QEMU 命令行
#
# 本文件被顶层 Makefile include，不直接执行。

CONFIG_MK := config/config.mk

ifeq ($(wildcard $(CONFIG_MK)),)
$(error 缺少 $(CONFIG_MK)。先生成配置：python tools/kconfig.py --defconfig config/defconfig-$(ARCH))
endif

include $(CONFIG_MK)

# ---------------------------------------------------------------------------
# 架构真相源统一到 config：ARCH 由已解析的配置反推，命令行只做一致性校验。
# ---------------------------------------------------------------------------
CONFIG_ARCH := $(if $(filter y,$(CONFIG_ARCH_LOONGARCH64)),loongarch64,riscv64)

ifneq ($(strip $(ARCH_REQUESTED)),)
ifneq ($(ARCH_REQUESTED),$(CONFIG_ARCH))
$(error ARCH='$(ARCH_REQUESTED)' 与 config/.config 里的架构选择 '$(CONFIG_ARCH)' 不一致。\
切换架构的完整动作是：make ARCH=$(ARCH_REQUESTED) defconfig clean build)
endif
endif

ARCH := $(CONFIG_ARCH)
export ARCH

# ---------------------------------------------------------------------------
# 架构派生值：切换 ARCH 必须连带切换 target / 链接脚本 / 链接基址 / QEMU 机器模型，
# 禁止只切 ARCH 而保留另一架构的已解析配置（docs/07 §2）。
# ---------------------------------------------------------------------------
ifeq ($(ARCH),riscv64)
RUST_TARGET    := riscv64gc-unknown-none-elf
LINKER_SCRIPT  := linker/riscv64.ld
LINK_BASE      := 0x80200000
QEMU_BIN       ?= qemu-system-riscv64
QEMU_MACHINE   := virt
QEMU_FIRMWARE  := -bios default
endif

ifeq ($(ARCH),loongarch64)
RUST_TARGET    := loongarch64-unknown-none
LINKER_SCRIPT  := linker/loongarch64.ld
LINK_BASE      := 0x00200000
QEMU_BIN       ?= qemu-system-loongarch64
QEMU_MACHINE   := virt
QEMU_FIRMWARE  :=
endif

ifeq ($(RUST_TARGET),)
$(error 未知 ARCH='$(ARCH)'，只支持 riscv64 / loongarch64)
endif

# ---------------------------------------------------------------------------
# Kconfig bool/choice → Cargo feature
# ---------------------------------------------------------------------------
RUST_FEATURES := arch-$(ARCH)

ifeq ($(CONFIG_LOG_LEVEL_TRACE),y)
LOG_LEVEL := trace
else ifeq ($(CONFIG_LOG_LEVEL_DEBUG),y)
LOG_LEVEL := debug
else ifeq ($(CONFIG_LOG_LEVEL_INFO),y)
LOG_LEVEL := info
else ifeq ($(CONFIG_LOG_LEVEL_WARN),y)
LOG_LEVEL := warn
else
LOG_LEVEL := error
endif

# cargo --features 只接受逗号分隔（--features a,b），空格会被当成位置参数。
RUST_FEATURES := $(RUST_FEATURES),log-level-$(LOG_LEVEL)

# ---------------------------------------------------------------------------
# Kconfig int/string → 脚本变量（运行时配置，不进 feature，见 Kconfig 顶部说明）
# ---------------------------------------------------------------------------
QEMU_SMP       := $(CONFIG_QEMU_SMP)
QEMU_MEMORY_MB := $(CONFIG_QEMU_MEMORY_MB)
# LoongArch QEMU virt 机器强制要求 ram_size > 1G（实测 < 1G 直接报错退出）。
# 在 build 阶段就拒绝，而不是等到 make run 才暴露。
ifeq ($(ARCH),loongarch64)
ifeq ($(shell test $(QEMU_MEMORY_MB) -ge 1024 && echo ok),)
$(error LoongArch QEMU virt 要求内存 >= 1G：CONFIG_QEMU_MEMORY_MB=$(QEMU_MEMORY_MB) < 1024，请调大后重新 defconfig)
endif
endif
KERNEL_CMDLINE := $(CONFIG_KERNEL_CMDLINE)
export KERNEL_MEMORY_MB := $(QEMU_MEMORY_MB)

# ---------------------------------------------------------------------------
# 产物与 Cargo 参数
# ---------------------------------------------------------------------------
PROFILE     ?= release
BUILD_DIR   := build/$(ARCH)
KERNEL_ELF  := $(BUILD_DIR)/kernel.elf
KERNEL_NAME := kernel

ifeq ($(PROFILE),release)
CARGO_PROFILE_FLAG := --release
CARGO_OUT_DIR      := target/$(RUST_TARGET)/release
else
CARGO_PROFILE_FLAG :=
CARGO_OUT_DIR      := target/$(RUST_TARGET)/debug
endif

export RUSTFLAGS := -C link-arg=-T$(LINKER_SCRIPT)

CARGO      ?= cargo
PYTHON     ?= python
QEMU_FLAGS := -machine $(QEMU_MACHINE) \
              -kernel $(KERNEL_ELF) \
              -smp $(QEMU_SMP) \
              -m $(QEMU_MEMORY_MB)M \
              -nographic \
              -serial mon:stdio \
              $(QEMU_FIRMWARE) \
              $(if $(KERNEL_CMDLINE),-append "$(KERNEL_CMDLINE)")
