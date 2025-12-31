# Makefile for user workspace
# - builds all binaries in src/bin into ELF (release, target: riscv64gc-unknown-none-elf)
# - make img: create 100MB image at ../kernel/disk.img, mkfs.ext4, mount and copy binaries to /test

TARGET := riscv64gc-unknown-none-elf
BUILD_DIR := target/$(TARGET)/release
BINS := $(patsubst src/bin/%.rs,%,$(wildcard src/bin/*.rs))
KERNEL_IMG := ../kernel/disk.img

.PHONY: all build img makeimg clean

all: build

build:
	cargo build --release --target $(TARGET)

img: build
	@echo "Creating $(KERNEL_IMG)..."
	dd if=/dev/zero of=$(KERNEL_IMG) bs=1M count=100
	# If the image is currently mounted or has a loop device, try to unmount/detach first
	@if mount | grep -q "$(KERNEL_IMG)" ; then \
		echo "Found existing mount for $(KERNEL_IMG); attempting to unmount..." ; \
		sudo umount $(KERNEL_IMG) || true ; \
	fi ; \
	# Detach any loop device associated with the image
	LOOPDEV=$$(losetup -j $(KERNEL_IMG) | cut -d: -f1) ; \
	if [ -n "$$LOOPDEV" ]; then \
		echo "Detaching loop device $$LOOPDEV" ; \
		sudo losetup -d $$LOOPDEV || true ; \
	fi ; \
	sudo mkfs.ext4 -O ^flex_bg,^metadata_csum -F $(KERNEL_IMG)
	TMPDIR=$$(mktemp -d) ; \
	echo "Mounting $(KERNEL_IMG) at $$TMPDIR" ; \
	sudo mount -o loop $(KERNEL_IMG) $$TMPDIR ; \
	sudo mkdir -p $$TMPDIR/test ; \
	for b in $(BINS); do \
		echo "Copying $$b..." ; \
		sudo cp $(BUILD_DIR)/$$b $$TMPDIR/test/ ; \
		sudo chmod +x $$TMPDIR/test/$$b ; \
	done ; \
	sync ; \
	sudo umount $$TMPDIR ; \
	rmdir $$TMPDIR
	
clean:
	cargo clean
	rm -f $(KERNEL_IMG)

