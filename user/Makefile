# Makefile for user workspace
# - builds all binaries in src/bin into ELF (release, target: riscv64gc-unknown-none-elf)
# - make img: create ../kernel/disk.img (GPT 4 partitions), format p1 ext4 / p2 FAT32 / p3 ext4 / p4 FAT32, and mark p1 as `rootfs`

ARCH ?= riscv64
TARGET := riscv64gc-unknown-none-elf
AARCH64_TARGET := aarch64-unknown-none-softfloat
RV_MUSL_GCC ?= $(firstword \
	$(wildcard /opt/riscv/bin/riscv64-unknown-linux-musl-gcc) \
	$(wildcard /opt/riscv/bin/riscv64-linux-musl-gcc) \
	$(shell command -v riscv64-unknown-linux-musl-gcc 2>/dev/null) \
	$(shell command -v riscv64-linux-musl-gcc 2>/dev/null))
AARCH64_MUSL_GCC := aarch64-linux-gnu-gcc
MUSL_SYSROOT ?= $(abspath $(dir $(RV_MUSL_GCC))../sysroot)
MUSL_CFLAGS ?= --sysroot=$(MUSL_SYSROOT) -I$(MUSL_SYSROOT)/include
CTEST := $(patsubst ctest/%.c,%,$(wildcard ctest/*.c))
CTEST_OUT_DIR := musltest
AARCH64_CTEST_OUT_DIR := musltest-aarch64
BUILD_DIR := target/$(TARGET)/release
AARCH64_BUILD_DIR := target/$(AARCH64_TARGET)/release
BINS := $(patsubst src/bin/%.rs,%,$(wildcard src/bin/*.rs))
KERNEL_IMG := ../kernel/disk.img
SDCARD_IMG := ../kernel/sdcard.img
OSCOMP_ELF_DIR := ../kernel/testsuits-for-oskernel/riscv-syscalls-testing/user/build/riscv64
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

.ONESHELL:

.PHONY: all build build-aarch64 build-doom img consent_img makeimg clean FORCE

ifeq ($(ARCH),aarch64)
IMG_BUILD_TARGET := build-aarch64
IMG_BIN_DIR := $(AARCH64_BUILD_DIR)
IMG_CTEST_OUT_DIR := $(AARCH64_CTEST_OUT_DIR)
else
IMG_BUILD_TARGET := build
IMG_BIN_DIR := $(BUILD_DIR)
IMG_CTEST_OUT_DIR := $(CTEST_OUT_DIR)
endif

DOOM_DIR := /home/inkbottle/othersrc/doomgeneric/doomgeneric
DOOM_MAKEFILE := $(DOOM_DIR)/Makefile.soso
DOOM_OUTPUT := fbdoom
DOOM_BINARY := $(DOOM_DIR)/$(DOOM_OUTPUT)
DOOM_WAD := $(DOOM_DIR)/doom1.wad
DOOM_STAGE_DIR := $(CURDIR)/mainfile
DOOM_CFLAGS := --sysroot=$(MUSL_SYSROOT) -march=rv64gc -mabi=lp64d
DOOM_LDFLAGS := --sysroot=$(MUSL_SYSROOT) -static -march=rv64gc -mabi=lp64d

all: build build-aarch64

build: FORCE
	@set -e; \
	cfg=$$(mktemp); \
	trap 'rm -f "$$cfg"' EXIT; \
	printf '[target.$(TARGET)]\nrustflags = [\n    "-Clink-args=-T$(CURDIR)/src/linker.ld",\n]\n\n' > "$$cfg"; \
	cargo build --config "$$cfg" --release --target $(TARGET)
	@mkdir -p $(CTEST_OUT_DIR)
	@test -n "$(RV_MUSL_GCC)" || { echo "error: no RISC-V musl C compiler found. expected one of: /opt/riscv/bin/riscv64-unknown-linux-musl-gcc, /opt/riscv/bin/riscv64-linux-musl-gcc, or one on PATH"; exit 127; }
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

# 交叉编译 Doomgeneric 的 BlueStarOS 版本，并把产物放进 `user/mainfile/`。
# 这样 `make img` 时会自动把 `fbdoom` 与 `doom1.wad` 一起塞进 rootfs `/mainfile/`。
build-doom: FORCE
	@set -e
	@test -n "$(RV_MUSL_GCC)" || { echo "error: no RISC-V musl C compiler found for doom build"; exit 127; }
	@test -f "$(DOOM_MAKEFILE)" || { echo "error: doomgeneric makefile not found: $(DOOM_MAKEFILE)"; exit 1; }
	@test -f "$(DOOM_WAD)" || { echo "error: doom wad not found: $(DOOM_WAD)"; exit 1; }
	$(MAKE) -C $(DOOM_DIR) -f Makefile.soso clean
	$(MAKE) -C $(DOOM_DIR) -f Makefile.soso \
		CC="$(RV_MUSL_GCC)" \
		LD="$(RV_MUSL_GCC)" \
		OUTPUT="$(DOOM_OUTPUT)" \
		CFLAGS="$(DOOM_CFLAGS)" \
		LDFLAGS="$(DOOM_LDFLAGS)"
	@mkdir -p "$(DOOM_STAGE_DIR)"
	$(HOST_CP) "$(DOOM_BINARY)" "$(DOOM_STAGE_DIR)/$(DOOM_OUTPUT)"
	$(HOST_CP) "$(DOOM_WAD)" "$(DOOM_STAGE_DIR)/doom1.wad"

img: build-doom $(IMG_BUILD_TARGET)
	@set -e
	@echo "Creating $(KERNEL_IMG)..."
	# 1) 创建一个 100MiB 的原始镜像文件：这里仅生成“裸磁盘”，不包含分区/文件系统
	dd if=/dev/zero of=$(KERNEL_IMG) bs=1M count=100
	# 2) 如果该镜像曾经被 losetup 绑定过，需要先 detach，否则后续分区扫描/格式化会用到旧映射
	LOOPDEV=$$($(LOSETUP) -j $(KERNEL_IMG) | cut -d: -f1)
	if [ -n "$$LOOPDEV" ]; then
		echo "Detaching loop device $$LOOPDEV"
		$(LOSETUP) -d $$LOOPDEV || true
	fi
	# 3) 写入 GPT 分区表，并为每个分区命名
	#    - p1: ext4 rootfs   [1MiB, 61MiB)
	#    - p2: FAT32        [61MiB, 71MiB)
	#    - p3: ext4         [71MiB, 86MiB)
	#    - p4: FAT32        [86MiB, end)
	#    这里改用 parted，因为当前环境下 sgdisk 对普通镜像文件会卡住。
	$(PARTED) -s $(KERNEL_IMG) mklabel gpt
	$(PARTED) -s $(KERNEL_IMG) mkpart rootfs ext4 1MiB 61MiB
	$(PARTED) -s $(KERNEL_IMG) name 1 rootfs
	$(PARTED) -s $(KERNEL_IMG) mkpart datafat fat32 61MiB 71MiB
	$(PARTED) -s $(KERNEL_IMG) name 2 datafat
	$(PARTED) -s $(KERNEL_IMG) mkpart dataext ext4 71MiB 86MiB
	$(PARTED) -s $(KERNEL_IMG) name 3 dataext
	$(PARTED) -s $(KERNEL_IMG) mkpart sharefat fat32 86MiB 100%
	$(PARTED) -s $(KERNEL_IMG) name 4 sharefat
	# 4) 把镜像绑定为 loop 设备，并让内核扫描 GPT 分区（-P 会创建 /dev/loopXp1..p4）
	LOOPDEV=$$($(LOSETUP) -Pf --show $(KERNEL_IMG))
	echo "Using loop device: $$LOOPDEV"
	# 5) 对每个分区单独 mkfs（mkfs 作用在分区范围上，而不是整盘）
	$(MKFS_EXT4) -b 4096 -O ^flex_bg,metadata_csum -F $${LOOPDEV}p1
	$(MKFS_VFAT) -F 32 $${LOOPDEV}p2
	$(MKFS_EXT4) -b 4096 -O ^flex_bg,metadata_csum -F $${LOOPDEV}p3
	$(MKFS_VFAT) -F 32 $${LOOPDEV}p4
	# 6) 只挂载第一个分区，把编译好的用户程序拷贝进去
	TMPDIR=$$(mktemp -d)
	TMPDIR2=$$(mktemp -d)
	FAT32_TXT=$$(mktemp)
	echo "Mounting $${LOOPDEV}p1 at $$TMPDIR"
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
	# 把 user/mainfile 目录中的全部内容放入 rootfs 的 /mainfile。
	# 这里使用 `cp -a source/. dest/`，这样普通文件、子目录以及隐藏文件都会被保留。
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
	if [ -d "$(OSCOMP_ELF_DIR)" ]; then
		echo "Copying oscomp test ELFs from $(OSCOMP_ELF_DIR)..."
		$(INSTALL) -d $$TMPDIR/oscomp
		$(INSTALL) -d $$TMPDIR2/oscomp

		for f in $(OSCOMP_ELF_DIR)/* ; do
			if [ -f "$$f" ]; then
				name=$$(basename $$f)
				echo "  Copying oscomp $$name..."
				$(INSTALL) -m 0755 "$$f" "$$TMPDIR/oscomp/$$name"
				$(INSTALL) -m 0755 "$$f" "$$TMPDIR2/oscomp/$$name"
			fi
		done
	else
		echo "Skip oscomp copy: $(OSCOMP_ELF_DIR) not found (build oscomp user first)."
	fi
	sync
	$(UMOUNT) $$TMPDIR
	$(UMOUNT) $$TMPDIR2
	rmdir $$TMPDIR
	rmdir $$TMPDIR2
	# 7) 收尾：detach loop 设备，避免影响下一次 make img
	$(LOSETUP) -d $$LOOPDEV

consent_img: build
	@set -e
	@echo "Creating $(SDCARD_IMG) (raw FAT32, no partition table)..."
	dd if=/dev/zero of=$(SDCARD_IMG) bs=1M count=128
	LOOPDEV=$$($(LOSETUP) -j $(SDCARD_IMG) | cut -d: -f1)
	if [ -n "$$LOOPDEV" ]; then
		echo "Detaching loop device $$LOOPDEV"
		$(LOSETUP) -d $$LOOPDEV || true
	fi
	LOOPDEV=$$($(LOSETUP) -f --show $(SDCARD_IMG))
	echo "Using loop device: $$LOOPDEV"
	$(MKFS_VFAT) -F 32 $$LOOPDEV
	TMPDIR=$$(mktemp -d)
	echo "Mounting $$LOOPDEV at $$TMPDIR"
	$(MOUNT) $$LOOPDEV $$TMPDIR
	if [ -d "$(OSCOMP_ELF_DIR)" ]; then
		echo "Copying oscomp test ELFs from $(OSCOMP_ELF_DIR) to FAT32 root..."
		for f in $(OSCOMP_ELF_DIR)/* ; do
			if [ -f "$$f" ]; then
				name=$$(basename $$f)
				echo "  Copyingc $$name..."
				$(INSTALL) -m 0755 "$$f" "$$TMPDIR/$$name"
			fi
		done
	else
		echo "Skip oscomp copy: $(OSCOMP_ELF_DIR) not found (build oscomp user first)."
	fi
	sync
	$(UMOUNT) $$TMPDIR
	rmdir $$TMPDIR
	$(LOSETUP) -d $$LOOPDEV

clean:
	cargo clean
	rm -rf $(CTEST_OUT_DIR) $(AARCH64_CTEST_OUT_DIR)
	rm -f $(KERNEL_IMG)
	rm -f $(SDCARD_IMG)
