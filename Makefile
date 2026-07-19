
KERNEL_DIR := kernel
TARGET := riscv64gc-unknown-none-elf
MODE := release

KERNEL_ELF := $(KERNEL_DIR)/target/$(TARGET)/$(MODE)/os
KERNEL_QEMU := kernel-qemu

BOOTLOADER_BIN := $(KERNEL_DIR)/bootloader/rustsbi-qemu.bin
SBI_QEMU := sbi-qemu

OBJCOPY := rust-objcopy
MKIMAGE ?= mkimage

SBI_LOAD_ADDR := 0x80000000
KERNEL_LOAD_ADDR ?= 0x80200000
KERNEL_ENTRY_ADDR ?= $(KERNEL_LOAD_ADDR)

DTB_DIR ?= $(abspath config/dtb)
DTB_NAME ?= $(notdir $(firstword $(wildcard $(DTB_DIR)/*.dtb)))
DTB_FILE ?= $(if $(DTB_NAME),$(DTB_DIR)/$(DTB_NAME),)
DTB_BASENAME := $(notdir $(DTB_FILE))

ITB_BUILD_DIR ?= $(abspath tmp/build/itb)
ITB_IMAGE := $(ITB_BUILD_DIR)/Image
ITB_ITS := $(ITB_BUILD_DIR)/bluestaros.its
ITB_OUT ?= $(ITB_BUILD_DIR)/bluestaros.itb

all: $(SBI_QEMU) $(KERNEL_QEMU)

.PHONY: user_build
user_build:
	@$(MAKE) -C user build

$(KERNEL_QEMU): user_build FORCE
	@$(MAKE) -C $(KERNEL_DIR) \
		TARGET=$(TARGET) MODE=$(MODE) \
		build
	@cp $(KERNEL_ELF) $(KERNEL_QEMU)

$(SBI_QEMU): FORCE
	@cp $(BOOTLOADER_BIN) $(SBI_QEMU)

$(ITB_IMAGE): $(KERNEL_QEMU)
	@mkdir -p $(ITB_BUILD_DIR)
	@$(OBJCOPY) -O binary $(KERNEL_ELF) $(ITB_IMAGE)
	@echo "Generated Linux-style kernel Image: $(ITB_IMAGE)"

$(ITB_ITS): $(ITB_IMAGE) FORCE
	@test -n "$(DTB_NAME)" -o -n "$(DTB_FILE)" || { echo "error: no DTB selected. Put *.dtb under $(DTB_DIR), pass DTB_NAME=<name>.dtb, or pass DTB_FILE=/path/to/file.dtb"; exit 2; }
	@test -f "$(DTB_FILE)" || { echo "error: DTB not found: $(DTB_FILE)"; exit 2; }
	@mkdir -p $(ITB_BUILD_DIR)
	@{ \
		echo '/dts-v1/;'; \
		echo ''; \
		echo '/ {'; \
		echo '    description = "BlueStarOS FIT image";'; \
		echo '    #address-cells = <2>;'; \
		echo ''; \
		echo '    images {'; \
		echo '        kernel {'; \
		echo '            description = "BlueStarOS kernel Image";'; \
		echo '            data = /incbin/("$(ITB_IMAGE)");'; \
		echo '            type = "kernel";'; \
		echo '            arch = "riscv";'; \
		echo '            os = "linux";'; \
		echo '            compression = "none";'; \
		echo '            load = <0x0 $(KERNEL_LOAD_ADDR)>;'; \
		echo '            entry = <0x0 $(KERNEL_ENTRY_ADDR)>;'; \
		echo '        };'; \
		echo ''; \
		echo '        fdt {'; \
		echo '            description = "$(DTB_BASENAME)";'; \
		echo '            data = /incbin/("$(DTB_FILE)");'; \
		echo '            type = "flat_dt";'; \
		echo '            arch = "riscv";'; \
		echo '            compression = "none";'; \
		echo '        };'; \
		echo '    };'; \
		echo ''; \
		echo '    configurations {'; \
		echo '        default = "conf";'; \
		echo '        conf {'; \
		echo '            description = "BlueStarOS with $(DTB_BASENAME)";'; \
		echo '            kernel = "kernel";'; \
		echo '            fdt = "fdt";'; \
		echo '        };'; \
		echo '    };'; \
		echo '};'; \
	} > $(ITB_ITS)
	@echo "Generated FIT source: $(ITB_ITS)"

.PHONY: itb
itb: $(ITB_ITS)
	@command -v $(MKIMAGE) >/dev/null 2>&1 || { echo "error: mkimage not found. Install u-boot-tools or set MKIMAGE=/path/to/mkimage"; exit 127; }
	@$(MKIMAGE) -f $(ITB_ITS) $(ITB_OUT)
	@echo "Generated ITB: $(ITB_OUT)"

.PHONY: all itb FORCE
