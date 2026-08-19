# BlueStarOS user-space build and rootfs image generation.

ARCH ?= riscv64
TARGET := riscv64gc-unknown-none-elf
AARCH64_TARGET := aarch64-unknown-none-softfloat

RV_MUSL_GCC ?= $(firstword \
	$(wildcard /opt/riscv/bin/riscv64-unknown-linux-musl-gcc) \
	$(wildcard /opt/riscv/bin/riscv64-linux-musl-gcc) \
	$(shell command -v riscv64-unknown-linux-musl-gcc 2>/dev/null) \
	$(shell command -v riscv64-linux-musl-gcc 2>/dev/null))
AARCH64_MUSL_GCC ?= aarch64-linux-gnu-gcc
MUSL_SYSROOT ?= $(abspath $(dir $(RV_MUSL_GCC))../sysroot)
MUSL_CFLAGS ?= --sysroot=$(MUSL_SYSROOT) -I$(MUSL_SYSROOT)/include

CTEST := $(patsubst ctest/%.c,%,$(wildcard ctest/*.c))
CTEST_OUT_DIR := musltest
AARCH64_CTEST_OUT_DIR := musltest-aarch64
BUILD_DIR := target/$(TARGET)/release
AARCH64_BUILD_DIR := target/$(AARCH64_TARGET)/release
BINS := $(patsubst src/bin/%.rs,%,$(wildcard src/bin/*.rs))

KERNEL_IMG := ../kernel/disk.img

SUDO ?= sudo -n
LOSETUP := $(SUDO) /usr/sbin/losetup
PARTED := $(SUDO) /usr/sbin/parted
MKFS_EXT4 := $(SUDO) /usr/sbin/mkfs.ext4
MKFS_VFAT := $(SUDO) /usr/sbin/mkfs.vfat
MOUNT := $(SUDO) /usr/bin/mount
UMOUNT := $(SUDO) /usr/bin/umount
INSTALL := $(SUDO) /usr/bin/install
CP := $(SUDO) /usr/bin/cp
HOST_CP := /usr/bin/cp

# Optional DoomGeneric integration. It is deliberately not part of the default
# image build so a fresh checkout does not depend on a developer-local path.
DOOM_DIR ?=
DOOM_OUTPUT ?= fbdoom
DOOM_MAKEFILE ?= $(if $(DOOM_DIR),$(DOOM_DIR)/Makefile.soso,)
DOOM_BINARY ?= $(if $(DOOM_DIR),$(DOOM_DIR)/$(DOOM_OUTPUT),)
DOOM_WAD ?= $(if $(DOOM_DIR),$(DOOM_DIR)/doom1.wad,)
DOOM_STAGE_DIR := $(CURDIR)/mainfile
DOOM_CFLAGS := --sysroot=$(MUSL_SYSROOT) -march=rv64gc -mabi=lp64d
DOOM_LDFLAGS := --sysroot=$(MUSL_SYSROOT) -static -march=rv64gc -mabi=lp64d

.ONESHELL:

.PHONY: all build build-aarch64 build-doom img clean FORCE

ifeq ($(ARCH),aarch64)
IMG_BUILD_TARGET := build-aarch64
IMG_BIN_DIR := $(AARCH64_BUILD_DIR)
IMG_CTEST_OUT_DIR := $(AARCH64_CTEST_OUT_DIR)
else
IMG_BUILD_TARGET := build
IMG_BIN_DIR := $(BUILD_DIR)
IMG_CTEST_OUT_DIR := $(CTEST_OUT_DIR)
endif

all: build build-aarch64

build: FORCE
	@set -e; \
	cfg=$$(mktemp); \
	trap 'rm -f "$$cfg"' EXIT; \
	printf '[target.$(TARGET)]\nrustflags = [\n    "-Clink-args=-T$(CURDIR)/src/linker.ld",\n]\n\n' > "$$cfg"; \
	cargo build --config "$$cfg" --release --target $(TARGET)
	@mkdir -p $(CTEST_OUT_DIR)
	@test -n "$(RV_MUSL_GCC)" || { echo "error: no RISC-V musl C compiler found; set RV_MUSL_GCC=/path/to/riscv64-unknown-linux-musl-gcc"; exit 127; }
	@for t in $(CTEST); do \
		echo "[ctest] CC $$t"; \
		$(RV_MUSL_GCC) $(MUSL_CFLAGS) -g -static -march=rv64gc -mabi=lp64d -o $(CTEST_OUT_DIR)/$$t ctest/$$t.c; \
		cp $(CTEST_OUT_DIR)/$$t ctest/$$t; \
	done

build-aarch64: FORCE
	@echo "Building aarch64 user binaries..."
	@set -e; \
	cfg=$$(mktemp); \
	trap 'rm -f "$$cfg"' EXIT; \
	printf '[target.$(AARCH64_TARGET)]\nrustflags = [\n    "-Clink-args=-T$(CURDIR)/src/linker-aarch64.ld",\n]\n\n' > "$$cfg"; \
	cargo build --config "$$cfg" --release --target $(AARCH64_TARGET)
	@mkdir -p $(AARCH64_CTEST_OUT_DIR)
	@for t in $(CTEST); do \
		echo "[ctest-aarch64] CC $$t"; \
		$(AARCH64_MUSL_GCC) -g -static -o $(AARCH64_CTEST_OUT_DIR)/$$t ctest/$$t.c; \
		cp $(AARCH64_CTEST_OUT_DIR)/$$t ctest/$$t; \
	done

build-doom: FORCE
	@set -e
	@test -n "$(DOOM_DIR)" || { echo "error: set DOOM_DIR=/path/to/doomgeneric"; exit 2; }
	@test -n "$(RV_MUSL_GCC)" || { echo "error: no RISC-V musl C compiler found for DoomGeneric"; exit 127; }
	@test -f "$(DOOM_MAKEFILE)" || { echo "error: DoomGeneric makefile not found: $(DOOM_MAKEFILE)"; exit 1; }
	@test -f "$(DOOM_WAD)" || { echo "error: Doom WAD not found: $(DOOM_WAD)"; exit 1; }
	$(MAKE) -C $(DOOM_DIR) -f $(notdir $(DOOM_MAKEFILE)) clean
	$(MAKE) -C $(DOOM_DIR) -f $(notdir $(DOOM_MAKEFILE)) \
		CC="$(RV_MUSL_GCC)" \
		LD="$(RV_MUSL_GCC)" \
		OUTPUT="$(DOOM_OUTPUT)" \
		CFLAGS="$(DOOM_CFLAGS)" \
		LDFLAGS="$(DOOM_LDFLAGS)"
	@mkdir -p "$(DOOM_STAGE_DIR)"
	$(HOST_CP) "$(DOOM_BINARY)" "$(DOOM_STAGE_DIR)/$(DOOM_OUTPUT)"
	$(HOST_CP) "$(DOOM_WAD)" "$(DOOM_STAGE_DIR)/doom1.wad"

img: $(IMG_BUILD_TARGET)
	@set -e
	@echo "Creating $(KERNEL_IMG)..."
	dd if=/dev/zero of=$(KERNEL_IMG) bs=1M count=100
	LOOPDEV=$$($(LOSETUP) -j $(KERNEL_IMG) | cut -d: -f1)
	if [ -n "$$LOOPDEV" ]; then
		echo "Detaching loop device $$LOOPDEV"
		$(LOSETUP) -d $$LOOPDEV || true
	fi

	$(PARTED) -s $(KERNEL_IMG) mklabel gpt
	$(PARTED) -s $(KERNEL_IMG) mkpart rootfs ext4 1MiB 61MiB
	$(PARTED) -s $(KERNEL_IMG) name 1 rootfs
	$(PARTED) -s $(KERNEL_IMG) mkpart datafat fat32 61MiB 71MiB
	$(PARTED) -s $(KERNEL_IMG) name 2 datafat
	$(PARTED) -s $(KERNEL_IMG) mkpart dataext ext4 71MiB 86MiB
	$(PARTED) -s $(KERNEL_IMG) name 3 dataext
	$(PARTED) -s $(KERNEL_IMG) mkpart sharefat fat32 86MiB 100%
	$(PARTED) -s $(KERNEL_IMG) name 4 sharefat

	LOOPDEV=$$($(LOSETUP) -Pf --show $(KERNEL_IMG))
	echo "Using loop device: $$LOOPDEV"
	trap '$(UMOUNT) "$${TMPDIR:-}" 2>/dev/null || true; $(UMOUNT) "$${TMPDIR2:-}" 2>/dev/null || true; [ -z "$$LOOPDEV" ] || $(LOSETUP) -d "$$LOOPDEV" 2>/dev/null || true; [ -z "$${TMPDIR:-}" ] || rmdir "$${TMPDIR}" 2>/dev/null || true; [ -z "$${TMPDIR2:-}" ] || rmdir "$${TMPDIR2}" 2>/dev/null || true' EXIT

	$(MKFS_EXT4) -b 4096 -O ^flex_bg,metadata_csum -F $${LOOPDEV}p1
	$(MKFS_VFAT) -F 32 $${LOOPDEV}p2
	$(MKFS_EXT4) -b 4096 -O ^flex_bg,metadata_csum -F $${LOOPDEV}p3
	$(MKFS_VFAT) -F 32 $${LOOPDEV}p4

	TMPDIR=$$(mktemp -d)
	TMPDIR2=$$(mktemp -d)
	FAT32_TXT=$$(mktemp)
	$(MOUNT) $${LOOPDEV}p1 $$TMPDIR
	$(MOUNT) $${LOOPDEV}p2 $$TMPDIR2
	printf "%s\n" hellofat32 > "$$FAT32_TXT"
	$(INSTALL) -m 0644 "$$FAT32_TXT" "$$TMPDIR2/fat32.txt"
	rm -f "$$FAT32_TXT"

	$(INSTALL) -d $$TMPDIR/test
	for b in $(BINS); do
		echo "Copying $$b..."
		$(INSTALL) -m 0755 $(IMG_BIN_DIR)/$$b $$TMPDIR/test/$$b
	done

	if [ -d "$(CURDIR)/mainfile" ]; then
		echo "Copying user/mainfile -> rootfs:/mainfile ..."
		$(INSTALL) -d $$TMPDIR/mainfile
		$(CP) -a "$(CURDIR)/mainfile/." "$$TMPDIR/mainfile/"
	fi

	if [ -d "$(IMG_CTEST_OUT_DIR)" ]; then
		$(INSTALL) -d $$TMPDIR/musltest
		for f in $(IMG_CTEST_OUT_DIR)/* ; do
			if [ -f "$$f" ]; then
				name=$$(basename $$f)
				echo "Copying ctest $$name..."
				$(INSTALL) -m 0755 "$$f" "$$TMPDIR/musltest/$$name"
			fi
		done
	fi

	sync
	$(UMOUNT) $$TMPDIR
	$(UMOUNT) $$TMPDIR2
	rmdir $$TMPDIR
	rmdir $$TMPDIR2
	$(LOSETUP) -d $$LOOPDEV
	trap - EXIT

clean:
	cargo clean
	rm -rf $(CTEST_OUT_DIR) $(AARCH64_CTEST_OUT_DIR)
	rm -f $(KERNEL_IMG)

FORCE:
