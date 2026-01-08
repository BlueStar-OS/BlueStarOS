# Makefile for user workspace
# - builds all binaries in src/bin into ELF (release, target: riscv64gc-unknown-none-elf)
# - make img: create ../kernel/disk.img (MBR 4 partitions), format p1 ext4 / p2 FAT16 / p3 FAT32 / p4 VFAT, then mount p1 and copy binaries to /test

TARGET := riscv64gc-unknown-none-elf
BUILD_DIR := target/$(TARGET)/release
BINS := $(patsubst src/bin/%.rs,%,$(wildcard src/bin/*.rs))
KERNEL_IMG := ../kernel/disk.img
SDCARD_IMG := ../kernel/sdcard.img
OSCOMP_ELF_DIR := ../kernel/testsuits-for-oskernel/riscv-syscalls-testing/user/build/riscv64

.ONESHELL:

.PHONY: all build img consent_img makeimg clean

all: build

build:
	cargo build --release --target $(TARGET)

img: build
	@echo "Creating $(KERNEL_IMG)..."
	# 1) 创建一个 100MiB 的原始镜像文件：这里仅生成“裸磁盘”，不包含分区/文件系统
	dd if=/dev/zero of=$(KERNEL_IMG) bs=1M count=100
	# 2) 如果该镜像曾经被 losetup 绑定过，需要先 detach，否则后续分区扫描/格式化会用到旧映射
	LOOPDEV=$$(losetup -j $(KERNEL_IMG) | cut -d: -f1)
	if [ -n "$$LOOPDEV" ]; then
		echo "Detaching loop device $$LOOPDEV"
		sudo losetup -d $$LOOPDEV || true
	fi
	# 3) 写入 MBR(dos) 分区表（使用扇区单位的旧式 sfdisk 输入格式，兼容性更好）
	printf '%s\n' \
		'2048,122880,83,*' \
		'124928,20480,0c' \
		'145408,30720,83' \
		'176128,28672,0c' \
	| sudo sfdisk $(KERNEL_IMG)
	# 4) 把镜像绑定为 loop 设备，并让内核扫描分区（-P 会创建 /dev/loopXp1..p4）
	LOOPDEV=$$(sudo losetup -Pf --show $(KERNEL_IMG))
	echo "Using loop device: $$LOOPDEV"
	# 5) 对每个分区单独 mkfs（mkfs 作用在分区范围上，而不是整盘）
	sudo mkfs.ext4 -O ^flex_bg,^metadata_csum -F $${LOOPDEV}p1
	sudo mkfs.vfat -F 32 $${LOOPDEV}p2
	sudo mkfs.ext4 -O ^flex_bg,^metadata_csum -F $${LOOPDEV}p3
	sudo mkfs.vfat -F 32 $${LOOPDEV}p4
	# 6) 只挂载第一个分区，把编译好的用户程序拷贝进去
	TMPDIR=$$(mktemp -d)
	TMPDIR2=$$(mktemp -d)
	echo "Mounting $${LOOPDEV}p1 at $$TMPDIR"
	sudo mount $${LOOPDEV}p1 $$TMPDIR
	sudo mount $${LOOPDEV}p2 $$TMPDIR2
	sudo sh -c 'printf "%s\n" hellofat32 > "'$$TMPDIR2'/fat32.txt"'
	sudo mkdir -p $$TMPDIR/test
	for b in $(BINS); do
		echo "Copying $$b..."
		sudo cp $(BUILD_DIR)/$$b $$TMPDIR/test/
		sudo chmod +x $$TMPDIR/test/$$b
	done
	if [ -d "$(OSCOMP_ELF_DIR)" ]; then
		echo "Copying oscomp test ELFs from $(OSCOMP_ELF_DIR)..."
		sudo mkdir -p $$TMPDIR/oscomp
		sudo mkdir -p $$TMPDIR2/oscomp

		for f in $(OSCOMP_ELF_DIR)/* ; do
			if [ -f "$$f" ]; then
				name=$$(basename $$f)
				echo "  Copying oscomp $$name..."
				sudo cp "$$f" "$$TMPDIR/oscomp/$$name"
				sudo cp "$$f" "$$TMPDIR2/oscomp/$$name"
				sudo chmod +x "$$TMPDIR/oscomp/$$name"
				sudo chmod +x "$$TMPDIR2/oscomp/$$name"
			fi
		done
	else
		echo "Skip oscomp copy: $(OSCOMP_ELF_DIR) not found (build oscomp user first)."
	fi
	sync
	sudo umount $$TMPDIR
	sudo umount $$TMPDIR2
	rmdir $$TMPDIR
	rmdir $$TMPDIR2
	# 7) 收尾：detach loop 设备，避免影响下一次 make img
	sudo losetup -d $$LOOPDEV

consent_img: build
	@echo "Creating $(SDCARD_IMG) (raw FAT32, no partition table)..."
	dd if=/dev/zero of=$(SDCARD_IMG) bs=1M count=128
	LOOPDEV=$$(losetup -j $(SDCARD_IMG) | cut -d: -f1)
	if [ -n "$$LOOPDEV" ]; then
		echo "Detaching loop device $$LOOPDEV"
		sudo losetup -d $$LOOPDEV || true
	fi
	LOOPDEV=$$(sudo losetup -f --show $(SDCARD_IMG))
	echo "Using loop device: $$LOOPDEV"
	sudo mkfs.vfat -F 32 $$LOOPDEV
	TMPDIR=$$(mktemp -d)
	echo "Mounting $$LOOPDEV at $$TMPDIR"
	sudo mount $$LOOPDEV $$TMPDIR
	if [ -d "$(OSCOMP_ELF_DIR)" ]; then
		echo "Copying oscomp test ELFs from $(OSCOMP_ELF_DIR) to FAT32 root..."
		for f in $(OSCOMP_ELF_DIR)/* ; do
			if [ -f "$$f" ]; then
				name=$$(basename $$f)
				echo "  Copying $$name..."
				sudo cp "$$f" "$$TMPDIR/$$name"
				sudo chmod +x "$$TMPDIR/$$name"
			fi
		done
	else
		echo "Skip oscomp copy: $(OSCOMP_ELF_DIR) not found (build oscomp user first)."
	fi
	sync
	sudo umount $$TMPDIR
	rmdir $$TMPDIR
	sudo losetup -d $$LOOPDEV

clean:
	cargo clean
	rm -f $(KERNEL_IMG)
	rm -f $(SDCARD_IMG)
