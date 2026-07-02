# BlueStarOS test system v1.
#
# This file is included by kernel/Makefile and intentionally keeps the kernel
# build path untouched. It only builds a dedicated test rootfs staging tree.

TESTOS_ROOT := $(abspath $(CURDIR)/..)
TESTOS_SRC_DIR := $(TESTOS_ROOT)/test
TESTOS_MODULES ?= syscall
TESTOS_BUILD_DIR := $(CURDIR)/target/testos
TESTOS_GEN_DIR := $(TESTOS_BUILD_DIR)/generated
TESTOS_BIN_DIR := $(TESTOS_BUILD_DIR)/bin
TESTOS_ROOTFS := $(TESTOS_BUILD_DIR)/rootfs
TESTOS_MANIFEST := $(TESTOS_ROOTFS)/test/manifest.txt
TESTOS_RUNNER_SRC := $(TESTOS_GEN_DIR)/test_all.c
TESTOS_RUNNER_BIN := $(TESTOS_BIN_DIR)/test_all
TESTOS_INITRAMFS := $(TESTOS_BUILD_DIR)/initramfs.cpio

TEST_CC_CANDIDATES := \
	/home/inkbottle/.local/riscv-musl/bin/riscv64-unknown-linux-musl-gcc \
	/opt/riscv/bin/riscv64-unknown-linux-musl-gcc \
	/opt/riscv/bin/riscv64-linux-musl-gcc \
	$(shell command -v riscv64-unknown-linux-musl-gcc 2>/dev/null) \
	$(shell command -v riscv64-linux-musl-gcc 2>/dev/null)
TEST_CC ?= $(firstword $(wildcard $(TEST_CC_CANDIDATES)))
TEST_SYSROOT ?= $(abspath $(dir $(TEST_CC))../sysroot)
TEST_CFLAGS ?= --sysroot=$(TEST_SYSROOT) -static -march=rv64gc -mabi=lp64d -O2 -g -Wall -Wextra
TEST_CPPFLAGS ?= -I$(TESTOS_SRC_DIR)/common
TEST_FILTER ?=
TEST_REBUILD_INITRAMFS ?= 0

TEST_SYSCALL_SRCS_ALL := $(sort $(wildcard $(TESTOS_SRC_DIR)/syscall/*.c))
ifneq ($(strip $(TEST_FILTER)),)
TEST_SYSCALL_SRCS := $(foreach src,$(TEST_SYSCALL_SRCS_ALL),$(if $(findstring $(TEST_FILTER),$(notdir $(src))),$(src)))
else
TEST_SYSCALL_SRCS := $(TEST_SYSCALL_SRCS_ALL)
endif
TEST_SYSCALL_BINS := $(patsubst $(TESTOS_SRC_DIR)/syscall/%.c,$(TESTOS_BIN_DIR)/syscall/%,$(TEST_SYSCALL_SRCS))

.PHONY: test test_all test_syscall testfs test_clean test_run_qemu test_check_tools test_check_syscall_sources test_initramfs test_pack_initramfs FORCE

test: test_all

test_all: testfs
	@echo "[testos] rootfs ready: $(TESTOS_ROOTFS)"
	@echo "[testos] TODO: QEMU execution is not wired in v1."
	@echo "[testos] TODO: after initramfs/rootfs loading lands, run /test/test_all inside BlueStarOS."

test_syscall: test_check_tools test_check_syscall_sources $(TEST_SYSCALL_BINS)
	@echo "[testos] built syscall tests: $(words $(TEST_SYSCALL_BINS))"

testfs: test_check_tools test_check_syscall_sources $(TESTOS_RUNNER_BIN) $(TEST_SYSCALL_BINS)
	@rm -rf "$(TESTOS_ROOTFS)"
	@mkdir -p "$(TESTOS_ROOTFS)/test/syscall" "$(TESTOS_ROOTFS)/dev" "$(TESTOS_ROOTFS)/tmp" "$(TESTOS_ROOTFS)/proc"
	@cp "$(TESTOS_RUNNER_BIN)" "$(TESTOS_ROOTFS)/test/test_all"
	@for bin in $(TEST_SYSCALL_BINS); do \
		name=$$(basename "$$bin"); \
		cp "$$bin" "$(TESTOS_ROOTFS)/test/syscall/$$name"; \
	done
	@{ \
		echo "# BlueStarOS test manifest"; \
		echo "# modules: $(TESTOS_MODULES)"; \
		for src in $(TEST_SYSCALL_SRCS); do \
			name=$$(basename "$$src" .c); \
			echo "syscall/$$name"; \
		done; \
	} > "$(TESTOS_MANIFEST)"
	@if [ "$(TEST_REBUILD_INITRAMFS)" = "1" ]; then \
		$(MAKE) -C "$(CURDIR)" test_pack_initramfs; \
	else \
		echo "[testos] skip initramfs cpio generation; set TEST_REBUILD_INITRAMFS=1 to build it"; \
	fi
	@echo "[testos] rootfs staging complete: $(TESTOS_ROOTFS)"

test_initramfs: testfs
	@$(MAKE) -C "$(CURDIR)" test_pack_initramfs

test_pack_initramfs:
	@command -v cpio >/dev/null 2>&1 || { echo "error: cpio is required to build $(TESTOS_INITRAMFS)"; exit 127; }
	@mkdir -p "$(dir $(TESTOS_INITRAMFS))"
	@cd "$(TESTOS_ROOTFS)" && find . -print | LC_ALL=C sort | cpio -o -H newc > "$(TESTOS_INITRAMFS)"
	@echo "[testos] initramfs cpio ready: $(TESTOS_INITRAMFS)"

test_run_qemu: testfs
	@echo "[testos] TODO: start QEMU, wait for shell, input /test/test_all, and parse [TESTOS] logs."
	@echo "[testos] TODO: keep this non-invasive; kernel initramfs/rootfs loading is implemented separately."

test_clean:
	@rm -rf "$(TESTOS_BUILD_DIR)"
	@echo "[testos] cleaned $(TESTOS_BUILD_DIR)"

test_check_tools:
	@test "$(ARCH)" = "riscv64" || { echo "error: TestOS v1 only supports ARCH=riscv64, got ARCH=$(ARCH)"; exit 2; }
	@test -n "$(TEST_CC)" || { echo "error: no RISC-V musl C compiler found; set TEST_CC=/path/to/riscv64-unknown-linux-musl-gcc"; exit 127; }
	@test -x "$(TEST_CC)" || { echo "error: TEST_CC is not executable: $(TEST_CC)"; exit 127; }

test_check_syscall_sources:
	@test -d "$(TESTOS_SRC_DIR)/syscall" || { echo "error: missing test/syscall directory"; exit 2; }
	@test -n "$(strip $(TEST_SYSCALL_SRCS))" || { echo "error: no syscall C tests matched in test/syscall"; exit 2; }

$(TESTOS_GEN_DIR):
	@mkdir -p "$@"

$(TESTOS_BIN_DIR)/syscall:
	@mkdir -p "$@"

$(TESTOS_RUNNER_SRC): FORCE $(TEST_SYSCALL_SRCS) $(TESTOS_SRC_DIR)/tools/gen_runner.py | $(TESTOS_GEN_DIR)
	@tmp="$@.tmp"; \
	python3 "$(TESTOS_SRC_DIR)/tools/gen_runner.py" \
		--module syscall \
		--output "$$tmp" \
		$(TEST_SYSCALL_SRCS); \
	if [ -f "$@" ] && cmp -s "$$tmp" "$@"; then \
		rm -f "$$tmp"; \
	else \
		mv "$$tmp" "$@"; \
	fi

$(TESTOS_RUNNER_BIN): $(TESTOS_RUNNER_SRC) | $(TESTOS_BIN_DIR)
	@echo "[testos] CC runner test_all"
	@$(TEST_CC) $(TEST_CFLAGS) $(TEST_CPPFLAGS) -o "$@" "$<"

$(TESTOS_BIN_DIR):
	@mkdir -p "$@"

$(TESTOS_BIN_DIR)/syscall/%: $(TESTOS_SRC_DIR)/syscall/%.c $(TESTOS_SRC_DIR)/common/testlib.h | $(TESTOS_BIN_DIR)/syscall
	@echo "[testos] CC syscall/$*"
	@$(TEST_CC) $(TEST_CFLAGS) $(TEST_CPPFLAGS) -o "$@" "$<"

FORCE:
