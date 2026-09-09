# BlueStarOS kernel build and FIT packaging.
#
# Board-specific code and driver selection live in Cargo.toml. Make only
# selects the board profile and packages the already-linked ELF.

BOARD ?= riscv64-qemu
MODE ?= release
SKIP_USER_IMAGE ?= 0
USER_DIR := ../user
PROJECT_DIR := $(abspath ..)

ifeq ($(BOARD),riscv64-qemu)
ARCH := riscv64
TARGET := riscv64gc-unknown-none-elf
QEMU_MACHINE ?= virt
# The DMA implementation uses CBO instructions directly.
QEMU_CPU ?= -cpu rv64,zicbom=true
QEMU_BIOS := ./bootloader/rustsbi-prototyper-jump.bin
QEMU_BIOS_ARG := -bios $(QEMU_BIOS)
QEMU_KERNEL = -kernel $(KERNEL_ELF)
QEMU_DEBUG_BOOT = -bios $(QEMU_BIOS) -device loader,file=$(KERNEL_BIN),addr=0x80200000
QEMU_SERIAL_ARGS := -chardev stdio,id=serial0,signal=off -serial chardev:serial0
GDB_SET_ARCH := -ex 'set arch riscv:rv64'
FIT_ARCH := riscv
QEMU_DEVICE_ARGS := \
	-drive file=disk.img,format=raw,if=none,id=nv0 \
	-drive file=emmc.img,format=raw,if=none,id=nv1 \
	-netdev tap,id=net0,ifname=tap0,script=no,downscript=no \
	-device pcie-root-port,id=rp0,bus=pcie.0,chassis=1,slot=1 \
	-device pcie-root-port,id=rp1,bus=pcie.0,chassis=1,slot=2 \
	-device pcie-root-port,id=rp2,bus=pcie.0,chassis=1,slot=3 \
	-device pcie-root-port,id=rp3,bus=pcie.0,chassis=1,slot=4 \
	-device pcie-root-port,id=rp4,bus=pcie.0,chassis=1,slot=5 \
	-device pcie-root-port,id=rp5,bus=pcie.0,chassis=1,slot=6 \
	-device edu,bus=rp0 \
	-device VGA,bus=rp1,id=video0 \
	-device nvme,serial=bsosnvme0,drive=nv0,bus=rp2 \
	-device nvme,serial=bsosnvme1,drive=nv1,bus=rp3 \
	-device e1000,bus=rp4,netdev=net0 \
	-device qemu-xhci,bus=rp5,id=xhci0 \
	-monitor unix:/tmp/qemu-monitor,server=on,wait=off
DEFAULT_DTB_SOURCE := src/dtb/virt-riscv/qemu-virt-riscv.dts
else ifeq ($(BOARD),spacemitk3-com260kit)
ARCH := riscv64
TARGET := riscv64gc-unknown-none-elf
FIT_ARCH := riscv
DEFAULT_DTB_SOURCE := src/dtb/spacemit_k3_com260kit/spacemit-k3-com260-ifx.dtb
else ifeq ($(BOARD),aarch64-qemu)
ARCH := aarch64
TARGET := aarch64-unknown-none-softfloat
QEMU_MACHINE ?= virt
QEMU_CPU ?= -cpu cortex-a72
QEMU_BIOS :=
QEMU_BIOS_ARG :=
QEMU_KERNEL = -kernel $(KERNEL_BIN)
QEMU_DEBUG_BOOT = $(QEMU_BIOS_ARG) $(QEMU_KERNEL)
QEMU_SERIAL_ARGS := -chardev stdio,id=serial0,signal=off -serial chardev:serial0
GDB_SET_ARCH := -ex 'set arch aarch64'
FIT_ARCH := arm64
QEMU_DEVICE_ARGS := \
	-machine virt,gic-version=3 \
	-drive file=emmc.img,format=raw,if=none,id=x0 \
	-net none \
	-device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0 \
	-device virtio-gpu-device,bus=virtio-mmio-bus.1
DEFAULT_DTB_SOURCE := src/dtb/virt-aarch64/qemu-virt-aarch64.dts
else
$(error Unsupported BOARD=$(BOARD). Supported: riscv64-qemu, spacemitk3-com260kit, aarch64-qemu)
endif

# QEMU xHCI tracing.  Keep this overridable, for example:
#   make QEMU_TRACE='-trace usb_xhci_fetch_trb' run
# QEMU writes the trace records to its stderr with the default log backend.
QEMU_TRACE ?= \
	-trace usb_xhci_reset \
	-trace usb_xhci_run \
	-trace usb_xhci_stop \
	-trace usb_xhci_cap_read \
	-trace usb_xhci_oper_read \
	-trace usb_xhci_oper_write \
	-trace usb_xhci_runtime_read \
	-trace usb_xhci_runtime_write \
	-trace usb_xhci_doorbell_read \
	-trace usb_xhci_doorbell_write \
	-trace usb_xhci_irq_intx \
	-trace usb_xhci_irq_msi \
	-trace usb_xhci_irq_msix \
	-trace usb_xhci_queue_event \
	-trace usb_xhci_fetch_trb \
	-trace pci_cfg_read \
	-trace pci_cfg_write \
	-trace pci_route_irq \
	-trace usb_port_claim \
	-trace usb_port_attach \
	-trace usb_port_detach \
	-trace usb_port_release \
	-trace usb_host_open_started \
	-trace usb_host_open_success \
	-trace usb_host_open_failure \
	-trace usb_host_close \
	-trace usb_xhci_port_link \
	-trace usb_xhci_port_notify \
	-trace usb_xhci_port_reset \
	-trace usb_xhci_port_read \
	-trace usb_xhci_port_write
ifeq ($(ARCH),riscv64)
RUSTFLAGS += -Ctarget-feature=+zicbom
export RUSTFLAGS
endif

FEATURES_ARG := --no-default-features --features $(BOARD)
KERNEL_ELF := target/$(TARGET)/$(MODE)/os
KERNEL_BIN := $(KERNEL_ELF).bin
DISASM_TMP := target/$(TARGET)/$(MODE)/asm

ifeq ($(MODE),release)
MODE_ARG := --release
endif

OBJCOPY ?= rust-objcopy
NM ?= rust-nm
READELF ?= readelf
MKIMAGE ?= mkimage
DTC ?= dtc

ITB_BUILD_DIR ?= $(PROJECT_DIR)/tmp/build/itb
ITB_IMAGE := $(ITB_BUILD_DIR)/Image
ITB_ITS := $(ITB_BUILD_DIR)/bluestaros.its
ITB_OUT ?= $(ITB_BUILD_DIR)/bluestaros.itb

DTB_FILE ?= $(abspath $(DEFAULT_DTB_SOURCE))
DTB_BASENAME := $(notdir $(DTB_FILE))
DTB_STAGE := $(ITB_BUILD_DIR)/$(DTB_BASENAME:.dts=.dtb)

# The FIT metadata is derived from the actual ELF entry. This keeps the
# linker script and U-Boot's load/entry fields mechanically consistent.
KERNEL_ENTRY_ADDR = $(shell $(READELF) -h $(KERNEL_ELF) 2>/dev/null | awk '/Entry point address:/ {print $$NF}')
KERNEL_ENTRY_ADDR_HI = $(shell printf '0x%x' $$(( $(KERNEL_ENTRY_ADDR) >> 32 )))
KERNEL_ENTRY_ADDR_LO = $(shell printf '0x%x' $$(( $(KERNEL_ENTRY_ADDR) & 0xffffffff )))

all: build

build:
	@if test "$(SKIP_USER_IMAGE)" = "0"; then $(MAKE) user-makeimg; fi
	@cargo build --target $(TARGET) $(MODE_ARG) $(FEATURES_ARG)
	@$(NM) --demangle -n $(KERNEL_ELF) 2>/dev/null | \
		grep ' [tT] ' | grep -v '^[0-9a-f]* t \.L' | \
		awk '{addr=$$1; $$1=""; $$2=""; gsub(/^ +/, ""); print addr " " $$0}' > src/kernel_symbols.txt
	@$(OBJCOPY) --binary-architecture=$(ARCH) $(KERNEL_ELF) --strip-all -O binary $(KERNEL_BIN)
	@cp $(KERNEL_ELF) src/BlueStarOS.elf
	@cp $(KERNEL_BIN) src/BlueStarOS.bin
	@echo "Copied to src/BlueStarOS.elf and src/BlueStarOS.bin"

user-makeimg:
	$(MAKE) -C $(USER_DIR) img ARCH=$(ARCH)
	@cp $(USER_DIR)/../kernel/disk.img $(USER_DIR)/../kernel/emmc.img
	@echo "Synced disk.img -> emmc.img"

user-consent-img:
	$(MAKE) -C $(USER_DIR) consent_img ARCH=$(ARCH)

$(DTB_STAGE): $(DTB_FILE)
	@mkdir -p $(ITB_BUILD_DIR)
	@if test "$(suffix $(DTB_FILE))" = ".dts"; then \
		command -v $(DTC) >/dev/null 2>&1 || { echo "error: dtc not found: $(DTC)"; exit 127; }; \
		$(DTC) -I dts -O dtb -o $@ $(DTB_FILE); \
	else \
		cp $(DTB_FILE) $@; \
	fi

$(ITB_IMAGE): build
	@mkdir -p $(ITB_BUILD_DIR)
	@$(OBJCOPY) --binary-architecture=$(ARCH) -O binary $(KERNEL_ELF) $@
	@test -s "$@" || { echo "error: objcopy did not produce $@"; exit 1; }
	@echo "Generated Linux-style kernel Image: $@"

$(ITB_ITS): $(ITB_IMAGE) $(DTB_STAGE)
	@test -n "$(KERNEL_ENTRY_ADDR)" || { echo "error: could not read ELF entry from $(KERNEL_ELF)"; exit 2; }
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
		echo '            arch = "$(FIT_ARCH)";'; \
		echo '            os = "linux";'; \
		echo '            compression = "none";'; \
		echo '            load = <$(KERNEL_ENTRY_ADDR_HI) $(KERNEL_ENTRY_ADDR_LO)>;'; \
		echo '            entry = <$(KERNEL_ENTRY_ADDR_HI) $(KERNEL_ENTRY_ADDR_LO)>;'; \
		echo '        };'; \
		echo ''; \
		echo '        fdt {'; \
		echo '            description = "$(DTB_BASENAME)";'; \
		echo '            data = /incbin/("$(DTB_STAGE)");'; \
		echo '            type = "flat_dt";'; \
		echo '            arch = "$(FIT_ARCH)";'; \
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
	} > $@
	@echo "Generated FIT source: $@"

.PHONY: itb
itb: SKIP_USER_IMAGE := 1
itb: $(ITB_ITS)
	@mkdir -p $(dir $(ITB_OUT))
	@if command -v $(MKIMAGE) >/dev/null 2>&1; then \
		$(MKIMAGE) -f $(ITB_ITS) $(ITB_OUT); \
	elif command -v $(DTC) >/dev/null 2>&1; then \
		echo "mkimage not found; compiling FIT source with $(DTC)"; \
		$(DTC) -I dts -O dtb -o $(ITB_OUT) $(ITB_ITS); \
	else \
		echo "error: neither mkimage nor dtc found"; exit 127; \
	fi
	@test -s "$(ITB_OUT)" || { echo "error: FIT image was not generated: $(ITB_OUT)"; exit 1; }
	@echo "Generated ITB: $(ITB_OUT)"

fmt-check:
	@cargo fmt --all --check

clippy-check:
	@cargo clippy --target $(TARGET) $(MODE_ARG) $(FEATURES_ARG) --all-targets -- -D warnings

env:
	(rustup target list | grep "$(TARGET) (installed)") || rustup target add $(TARGET)
	rustup component add rust-src
	rustup component add llvm-tools-preview

clean:
	@cargo clean
	@cd $(USER_DIR) && cargo clean

# QEMU's user-mode network backend expects a host tap device. Keep this
# idempotent: ask for sudo once, create tap0 only when absent, and bring it up
# when it exists but is down.
.PHONY: setup-tap
setup-tap:
	@sudo -v
	@if ! ip link show dev tap0 >/dev/null 2>&1; then \
		sudo ip tuntap add dev tap0 mode tap; \
	fi
	@if ! ip link show dev tap0 2>/dev/null | grep -Eq '<(UP|[^>]*,UP)(,|>)'; then \
		sudo ip link set dev tap0 up; \
	fi

ifeq ($(filter %qemu,$(BOARD)),)
run debug gdbserver:
	$(error $(BOARD) has no local QEMU run target)
else
run: setup-tap build
	@$(QEMU_PLAT)qemu-system-$(ARCH) -machine $(QEMU_MACHINE) $(QEMU_CPU) -m 256m \
		$(QEMU_BIOS_ARG) $(QEMU_KERNEL) $(QEMU_DTB_ARG) $(QEMU_TRACE) -display gtk -monitor none \
		$(QEMU_SERIAL_ARGS) -s $(QEMU_DEVICE_ARGS)

debug: setup-tap build
	@tmux new-session -d "$(QEMU_PLAT)qemu-system-$(ARCH) -machine $(QEMU_MACHINE) $(QEMU_CPU) -m 256m -nographic -monitor none $(QEMU_DEBUG_BOOT) $(QEMU_DTB_ARG) $(QEMU_TRACE) $(QEMU_SERIAL_ARGS) $(QEMU_DEVICE_ARGS) -s -S" && \
		tmux split-window -h "$(ARCH)-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' $(GDB_SET_ARCH) -ex 'target remote localhost:1234'" && \
		tmux -2 attach-session -d

gdbserver: setup-tap build
	@$(QEMU_PLAT)qemu-system-$(ARCH) -machine $(QEMU_MACHINE) $(QEMU_CPU) -m 256m -nographic \
		-monitor none $(QEMU_DEBUG_BOOT) $(QEMU_DTB_ARG) $(QEMU_TRACE) $(QEMU_SERIAL_ARGS) $(QEMU_DEVICE_ARGS) -s
endif

gdbclient:
	@$(ARCH)-unknown-elf-gdb -ex 'file $(KERNEL_ELF)' $(GDB_SET_ARCH) -ex 'target remote localhost:1234'

.PHONY: all build fmt-check clippy-check env clean user-makeimg user-consent-img \
	setup-tap \
	run debug gdbserver gdbclient

-include TestOS.mk
