
KERNEL_DIR := kernel
TARGET := riscv64gc-unknown-none-elf
MODE := release

KERNEL_ELF := $(KERNEL_DIR)/target/$(TARGET)/$(MODE)/os
KERNEL_QEMU := kernel-qemu

BOOTLOADER_BIN := $(KERNEL_DIR)/bootloader/rustsbi-qemu.bin
SBI_QEMU := sbi-qemu

OBJCOPY := rust-objcopy

SBI_LOAD_ADDR := 0x80000000

all: $(SBI_QEMU) $(KERNEL_QEMU)

$(KERNEL_QEMU): FORCE
	@$(MAKE) -C $(KERNEL_DIR) \
		TARGET=$(TARGET) MODE=$(MODE) \
		build
	@cp $(KERNEL_ELF) $(KERNEL_QEMU)

$(SBI_QEMU): FORCE
	@cp $(BOOTLOADER_BIN) $(SBI_QEMU)

.PHONY: all FORCE
