# Building
ARCH  ?= riscv64
MODE := release
QEMU_MACHINE ?= virt
QEMU_PLAT ?=
QEMU_7.2 := /opt/qemu-7.2/bin/
USER_DIR := ../user
LOG ?= DEBUG

# Architecture-specific settings
ifeq ($(ARCH), riscv64)
TARGET := riscv64gc-unknown-none-elf
KERNEL_ENTRY_PA := 0x80200000
BOOTLOADER := ./bootloader/rustsbi-prototyper-jump.bin
QEMU_CPU :=
QEMU_BIOS := 
QEMU_KERNEL = -kernel $(KERNEL_ELF)
QEMU_DEBUG_BOOT = -bios $(BOOTLOADER) -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA)
GDB_SET_ARCH = -ex 'set arch riscv:rv64'
# 驱动 features
DRIVER_FEATURES ?= qemu_plat

	
QEMU_DEVICE_ARGS = \
	-drive file=disk.img,format=raw,if=none,id=x0 \
	-drive file=emmc.img,format=raw,if=none,id=x1 \
	-net none \
	-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
	-device virtio-blk-device,drive=x1,bus=virtio-mmio-bus.2 \
	-device pcie-root-port,id=rp0,bus=pcie.0,chassis=1,slot=1 \
	-device pcie-root-port,id=rp1,bus=pcie.0,chassis=1,slot=2 \
	-device edu,bus=rp0 \
	-device VGA,bus=rp1,id=video0
else ifeq ($(ARCH), aarch64)
TARGET := aarch64-unknown-none-softfloat
KERNEL_ENTRY_PA := 0x40080000
BOOTLOADER :=
QEMU_CPU := -cpu cortex-a72
QEMU_BIOS :=
QEMU_KERNEL = -kernel $(KERNEL_BIN)
QEMU_DEBUG_BOOT = $(QEMU_BIOS) $(QEMU_KERNEL)
GDB_SET_ARCH = -ex 'set arch aarch64'
# 驱动 features
DRIVER_FEATURES ?= qemu_plat


QEMU_DEVICE_ARGS = \
	-machine virt,gic-version=3  \
	-drive file=emmc.img,format=raw,if=none,id=x0 \
	-net none \
	-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
	-device virtio-gpu-device,bus=virtio-mmio-bus.1 


else
$(error Unsupported ARCH=$(ARCH). Supported: riscv64, aarch64)
endif

KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin
DISASM_TMP := target/$(TARGET)/$(MODE)/asm

# Building mode argument
ifeq ($(MODE), release)
	MODE_ARG := --release
endif

# Driver features argument
ifneq ($(DRIVER_FEATURES),)
	FEATURES_ARG := --features "$(DRIVER_FEATURES)"
else
	FEATURES_ARG :=
endif

ifneq ($(QEMU_DTB),)
	QEMU_DTB_ARG := -dtb $(QEMU_DTB)
else
	QEMU_DTB_ARG :=
endif



# Binutils
OBJDUMP := rust-objdump --arch-name=$(ARCH)
OBJCOPY := rust-objcopy --binary-architecture=$(ARCH)

# Disassembly
DISASM ?= -x

# 总是重新构建：每次 build 都跑 cargo build 和 objcopy
build: user-makeimg
	@cargo clean
	@rm -f $(KERNEL_ELF) $(KERNEL_BIN) src/BlueStarOS.elf src/BlueStarOS.bin
	@LOG=$(LOG) cargo build --target $(TARGET) $(MODE_ARG) $(FEATURES_ARG)
	@# 从 ELF 提取内核符号表，生成 src/kernel_symbols.txt（用于 backtrace 函数名解析）
	@rust-nm --demangle -n $(KERNEL_ELF) 2>/dev/null | grep ' [tT] ' | grep -v '^[0-9a-f]* t \.L' | awk '{addr=$$1; $$1=""; $$2=""; gsub(/^ +/, ""); print addr " " $$0}' > src/kernel_symbols.txt
	$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $(KERNEL_BIN)
	@cp $(KERNEL_ELF) src/BlueStarOS.elf
	@cp $(KERNEL_BIN) src/BlueStarOS.bin
	@echo "Copied to src/BlueStarOS.elf and src/BlueStarOS.bin"

user-makeimg:
	$(MAKE) -C $(USER_DIR) img ARCH=$(ARCH)
	@cp $(USER_DIR)/../kernel/disk.img $(USER_DIR)/../kernel/emmc.img
	@echo "Synced disk.img -> emmc.img"

user-consent-img:
	$(MAKE) -C $(USER_DIR) consent_img ARCH=$(ARCH)

env:
	(rustup target list | grep "$(TARGET) (installed)") || rustup target add $(TARGET)
	cargo install cargo-binutils
	rustup component add rust-src
	rustup component add llvm-tools-preview


clean:
	@echo "==> Cleaning build artifacts..."
	@cargo clean
	@cd $(USER_DIR) && cargo clean
	@rm -f $(APP_ASM)
	@echo "✓ Clean complete"


run: run-inner

run_consent: run-consent-inner

# run-inner: 保留图形窗口，同时把 serial0 绑定到当前终端
run-inner: build
	@$(QEMU_PLAT)$(QEMU_7.2)qemu-system-$(ARCH) \
		-machine $(QEMU_MACHINE) \
		$(QEMU_CPU) \
		-m 256m \
		$(QEMU_BIOS) \
		$(QEMU_KERNEL) \
		$(QEMU_DTB_ARG) \
		-display gtk \
		-monitor none \
		-serial stdio \
		-s \
		$(QEMU_DEVICE_ARGS) \

run-consent-inner: user-consent-img
	@cargo build --target $(TARGET) $(MODE_ARG) $(FEATURES_ARG)
	@$(QEMU_PLAT)qemu-system-$(ARCH) --version \
	
	@$(QEMU_PLAT)qemu-system-$(ARCH) \
		-machine virt \
		-kernel $(KERNEL_BIN) \
		$(QEMU_DTB_ARG) \
		-m 128M \
		-nographic \
		-smp 2 \
		-bios $(BOOTLOADER) \
		-drive file=sdcard.img,if=none,format=raw,id=x0 \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
		-device virtio-net-device,netdev=net \
		-netdev user,id=net

debug: build
	@tmux new-session -d \
		"$(QEMU_PLAT)qemu-system-$(ARCH) -machine $(QEMU_MACHINE) $(QEMU_CPU) -m 256m -nographic $(QEMU_DEBUG_BOOT) $(QEMU_DTB_ARG) $(QEMU_DEVICE_ARGS) -s -S" && \
		tmux split-window -h "$(ARCH)-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' $(GDB_SET_ARCH) -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

gdbserver: build
	@$(QEMU_PLAT)qemu-system-$(ARCH) -machine $(QEMU_MACHINE) $(QEMU_CPU) -m 256m -nographic $(QEMU_DEBUG_BOOT) $(QEMU_DTB_ARG) $(QEMU_DEVICE_ARGS) -s 

gdbclient:
	@$(ARCH)-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' $(GDB_SET_ARCH) -ex 'target remote localhost:1234'

.PHONY: build env kernel clean user user-apps disasm disasm-vim run-inner run-consent-inner run_consent gdbserver gdbclient list-apps regen-asm
