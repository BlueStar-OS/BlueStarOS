# Building
TARGET := riscv64gc-unknown-none-elf
MODE := release
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin
DISASM_TMP := target/$(TARGET)/$(MODE)/asm
USER_DIR := ../user

# Building mode argument
ifeq ($(MODE), release)
	MODE_ARG := --release
endif

# BOARD
BOARD := qemu
SBI ?= rustsbi
BOOTLOADER := ./bootloader/$(SBI)-$(BOARD).bin

# KERNEL ENTRY
KERNEL_ENTRY_PA := 0x80200000

# Binutils
OBJDUMP := rust-objdump --arch-name=riscv64
OBJCOPY := rust-objcopy --binary-architecture=riscv64

# Disassembly
DISASM ?= -x

# 总是重新构建：每次 build 都跑 cargo build 和 objcopy
build: user-makeimg
	cargo build --target $(TARGET) $(MODE_ARG)
	$(OBJCOPY) $(KERNEL_ELF) --strip-all -O binary $(KERNEL_BIN)

user-makeimg:
	$(MAKE) -C $(USER_DIR) img

user-consent-img:
	$(MAKE) -C $(USER_DIR) consent_img

env:
	(rustup target list | grep "riscv64gc-unknown-none-elf (installed)") || rustup target add $(TARGET)
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

run-inner: build
	@qemu-system-riscv64 \
		-machine virt \
		-nographic \
		-m 256m \
		-bios $(BOOTLOADER) \
		-device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA) -s \
		-drive file=disk.img,format=raw,if=none,id=x0 \
		-net none \
		-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \

run-consent-inner: user-consent-img
	@cargo build --target $(TARGET) $(MODE_ARG)
	@qemu-system-riscv64 \
		-machine virt \
		-kernel $(KERNEL_ELF) \
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
		"qemu-system-riscv64 -machine virt -nographic -bios $(BOOTLOADER) -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA) -s -S" && \
		tmux split-window -h "riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

gdbserver: build
	@qemu-system-riscv64 -machine virt -nographic -bios $(BOOTLOADER) -device loader,file=$(KERNEL_BIN),addr=$(KERNEL_ENTRY_PA) -s 

gdbclient:
	@riscv64-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' -ex 'set arch riscv:rv64' -ex 'target remote localhost:1234'

.PHONY: build env kernel clean user user-apps disasm disasm-vim run-inner run-consent-inner run_consent gdbserver gdbclient list-apps regen-asm
