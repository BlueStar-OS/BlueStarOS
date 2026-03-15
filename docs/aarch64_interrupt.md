# AArch64 中断机制完全指南 —— 面向 BlueStarOS (RK3588)

> 本文档提供足够的细节让你从零手写 AArch64 平台的中断子系统。
> 所有寄存器偏移、位域均来自 Linux 5.4.29 / WSL2-Linux-Kernel 源码和 ARM 架构手册。
> RK3588 地址来自 `arch/arm64/boot/dts/rockchip/rk3588s.dtsi`。

---

## 目录

1. [架构对比：RISC-V vs AArch64](#1-架构对比)
2. [AArch64 异常模型](#2-aarch64-异常模型)
3. [异常向量表（VBAR_EL1）](#3-异常向量表)
4. [DAIF 中断屏蔽](#4-daif-中断屏蔽)
5. [GICv3 架构总览](#5-gicv3-架构总览)
6. [RK3588 GIC 地址映射](#6-rk3588-gic-地址映射)
7. [Distributor (GICD) 初始化](#7-distributor-初始化)
8. [Redistributor (GICR) 初始化](#8-redistributor-初始化)
9. [CPU Interface (ICC) 初始化](#9-cpu-interface-初始化)
10. [中断处理流程（claim / handle / EOI）](#10-中断处理流程)
11. [内核态中断处理](#11-内核态中断处理)
12. [UART 中断接入](#12-uart-中断接入)
13. [完整初始化伪代码](#13-完整初始化伪代码)
14. [Linux 源码参考索引](#14-linux-源码参考索引)

---

## 1. 架构对比

```
RISC-V                              AArch64
─────────────────────────────────   ─────────────────────────────────
M-mode / S-mode / U-mode            EL3 / EL2 / EL1 / EL0
stvec (trap向量，单入口)              VBAR_EL1 (向量表，16个入口)
sstatus.SIE (全局中断开关)           DAIF.I (IRQ屏蔽位)
sie 寄存器 (SEIE/STIE/SSIE)         ICC_IGRPEN1_EL1 (Group1使能)
scause (异常原因)                    ESR_EL1 (同步异常原因)
sepc (异常返回地址)                  ELR_EL1 (异常返回地址)
PLIC (外部中断控制器)                GICv3 (通用中断控制器)
  - 单一claim/complete               - GICD + GICR + ICC 三级结构
  - MMIO 接口                        - GICD/GICR用MMIO, ICC用系统寄存器
ecall (系统调用)                     svc (系统调用)
sret (异常返回)                      eret (异常返回)
wfi (等待中断)                       wfi (等待中断)
```

核心区别：
- RISC-V 的 PLIC 是纯 MMIO，claim 一个寄存器搞定
- AArch64 的 GICv3 分三层：Distributor(全局) + Redistributor(per-CPU) + CPU Interface(系统寄存器)
- RISC-V 只有一个 trap 入口（stvec），AArch64 有 16 个向量入口
- RISC-V 用 `sstatus.SIE` 全局开关，AArch64 用 `PSTATE.I`（DAIF 的 I 位）

---

## 2. AArch64 异常模型

### 2.1 Exception Level（特权级）

```
EL3  ─── Secure Monitor (ATF/TF-A)     ← 你不碰这层
EL2  ─── Hypervisor                     ← 启动时经过，降到 EL1
EL1  ─── OS Kernel (BlueStarOS)        ← 你的代码运行在这里
EL0  ─── User Application               ← 用户进程
```

### 2.2 异常类型

AArch64 把异常分为 4 类：

| 类型 | 触发方式 | 例子 |
|------|---------|------|
| Synchronous | 指令执行触发 | SVC(系统调用), 页错误, 未定义指令 |
| IRQ | 外部中断线 | GIC 路由的所有设备中断、定时器中断 |
| FIQ | 快速中断线 | 通常被 EL3 (ATF) 占用，OS 不用 |
| SError | 异步系统错误 | 总线错误、ECC 错误 |

### 2.3 关键系统寄存器

| 寄存器 | 用途 | 对应 RISC-V |
|--------|------|------------|
| `VBAR_EL1` | 异常向量表基地址 | `stvec` |
| `ELR_EL1` | 异常返回地址 | `sepc` |
| `SPSR_EL1` | 保存的 PSTATE | `sstatus` |
| `ESR_EL1` | 异常综合信息（EC+ISS） | `scause` + `stval` |
| `FAR_EL1` | 错误虚拟地址 | `stval` |
| `SP_EL0` | EL0 栈指针 | 用户 sp |
| `DAIF` | 中断屏蔽标志 | `sstatus.SIE` |
| `TPIDR_EL1` | 内核线程指针（你用来存 TrapContext 地址） | `sscratch` |

---

## 3. 异常向量表

### 3.1 向量表布局

VBAR_EL1 指向一个 2048 字节对齐的表，包含 16 个入口，每个 128 字节（`.align 7`）：

```
偏移        来源                    使用的SP        异常类型
────────────────────────────────────────────────────────────
0x000       当前EL (EL1)           SP_EL0          Synchronous
0x080       当前EL (EL1)           SP_EL0          IRQ
0x100       当前EL (EL1)           SP_EL0          FIQ
0x180       当前EL (EL1)           SP_EL0          SError

0x200       当前EL (EL1)           SP_ELx          Synchronous   ← 内核态异常
0x280       当前EL (EL1)           SP_ELx          IRQ           ← 内核态IRQ ★
0x300       当前EL (EL1)           SP_ELx          FIQ
0x380       当前EL (EL1)           SP_ELx          SError

0x400       低EL (EL0, AArch64)    -               Synchronous   ← 用户态syscall
0x480       低EL (EL0, AArch64)    -               IRQ           ← 用户态IRQ ★
0x500       低EL (EL0, AArch64)    -               FIQ
0x580       低EL (EL0, AArch64)    -               SError

0x600       低EL (EL0, AArch32)    -               Synchronous
0x680       低EL (EL0, AArch32)    -               IRQ
0x700       低EL (EL0, AArch32)    -               FIQ
0x780       低EL (EL0, AArch32)    -               SError
```

### 3.2 你需要关注的入口

对于中断处理，核心是这两个：
- `0x280` — EL1h IRQ：内核态收到 IRQ（对应 RISC-V 的 kernel_mode_trap_handler）
- `0x480` — EL0 64-bit IRQ：用户态收到 IRQ（对应 RISC-V 的 kernel_trap_handler 中 SupervisorExternal 分支）

### 3.3 Linux 的向量表实现

来自 `arch/arm64/kernel/entry.S:520`（WSL2-Linux-Kernel）：

```asm
SYM_CODE_START(vectors)
    kernel_ventry  1, t, 64, sync       // EL1t Sync
    kernel_ventry  1, t, 64, irq        // EL1t IRQ
    kernel_ventry  1, t, 64, fiq        // EL1t FIQ
    kernel_ventry  1, t, 64, error      // EL1t SError

    kernel_ventry  1, h, 64, sync       // EL1h Sync    ← 内核态同步异常
    kernel_ventry  1, h, 64, irq        // EL1h IRQ     ← 内核态IRQ ★
    kernel_ventry  1, h, 64, fiq        // EL1h FIQ
    kernel_ventry  1, h, 64, error      // EL1h SError

    kernel_ventry  0, t, 64, sync       // EL0 Sync     ← 用户态syscall
    kernel_ventry  0, t, 64, irq        // EL0 IRQ      ← 用户态IRQ ★
    kernel_ventry  0, t, 64, fiq        // EL0 FIQ
    kernel_ventry  0, t, 64, error      // EL0 SError
    ...
SYM_CODE_END(vectors)
```

每个 `kernel_ventry` 宏展开后：保存寄存器 → 调用 C handler → 恢复寄存器 → `eret`

### 3.4 你的 BlueStarOS 现有向量表

你的 `trap.asm` 已经有了基本框架，但 `irq_el1_spx` 目前直接 panic。
需要改为：保存 caller-saved 寄存器 → 调用 GIC handler → 恢复 → `eret`。

---

## 4. DAIF 中断屏蔽

PSTATE 中的 DAIF 四个位控制异常屏蔽：

```
bit 9 (D) — Debug exceptions
bit 8 (A) — SError (Asynchronous abort)
bit 7 (I) — IRQ                          ← 这是你的中断开关
bit 6 (F) — FIQ
```

### 4.1 操作指令

```asm
// 关中断（设置 I 位 = 屏蔽 IRQ）—— 等价于 RISC-V clear_sie()
msr daifset, #2        // 设置 bit 7 (I)

// 开中断（清除 I 位 = 允许 IRQ）—— 等价于 RISC-V set_sie()
msr daifclr, #2        // 清除 bit 7 (I)

// 全部屏蔽
msr daifset, #0xf      // D|A|I|F 全部设置

// 全部开启
msr daifclr, #0xf      // D|A|I|F 全部清除
```

### 4.2 Rust 中的用法

```rust
// 关 IRQ
unsafe { core::arch::asm!("msr daifset, #2"); }

// 开 IRQ
unsafe { core::arch::asm!("msr daifclr, #2"); }

// 读取 DAIF
let daif: u64;
unsafe { core::arch::asm!("mrs {}, daif", out(reg) daif); }
let irq_masked = (daif >> 7) & 1;  // 1 = IRQ被屏蔽
```

### 4.3 与 SPSR_EL1 的关系

当异常发生时，CPU 自动：
1. `SPSR_EL1 = PSTATE`（保存当前状态，包括 DAIF）
2. `PSTATE.I = 1`（自动屏蔽 IRQ）
3. `PSTATE.F = 1`（自动屏蔽 FIQ）

`eret` 时自动：
1. `PSTATE = SPSR_EL1`（恢复，包括 DAIF）

这和 RISC-V 的 `sstatus.SIE → SPIE` 机制完全对应：
- 进 trap 时 SIE→SPIE, SIE=0（关中断）
- sret 时 SPIE→SIE（恢复中断）

### 4.4 内核态开中断（关键！）

和 RISC-V 一样的问题：内核态默认 `PSTATE.I=1`（IRQ 屏蔽）。
要在内核态等待中断（如 get_char 的 wfi），需要临时开 IRQ：

```rust
// 等价于 RISC-V 的 set_sie() + wfi + clear_sie()
unsafe {
    core::arch::asm!("msr daifclr, #2");  // 开 IRQ
    core::arch::asm!("wfi");               // 等待中断
    core::arch::asm!("msr daifset, #2");  // 关 IRQ
}
```

---

## 5. GICv3 架构总览

GICv3 是 ARM 的通用中断控制器，对应 RISC-V 的 PLIC，但复杂得多。

### 5.1 三级结构

```
                    ┌─────────────────────────────────────────┐
                    │           Distributor (GICD)             │
                    │         全局唯一，管理 SPI 中断           │
                    │    MMIO base: 0xfe600000 (RK3588)       │
                    └──────────┬──────────────┬───────────────┘
                               │              │
                    ┌──────────▼──┐    ┌──────▼──────────┐
                    │ Redistributor│    │ Redistributor    │
                    │   (GICR)     │    │   (GICR)         │
                    │  per-CPU     │    │  per-CPU         │
                    │  管理SGI/PPI │    │  管理SGI/PPI     │
                    │  0xfe680000  │    │  +0x20000        │
                    └──────┬──────┘    └──────┬───────────┘
                           │                  │
                    ┌──────▼──────┐    ┌──────▼───────────┐
                    │ CPU Interface│    │ CPU Interface     │
                    │   (ICC_*)    │    │   (ICC_*)         │
                    │  系统寄存器   │    │  系统寄存器       │
                    │  per-CPU     │    │  per-CPU          │
                    └──────┬──────┘    └──────┬───────────┘
                           │                  │
                      ┌────▼────┐        ┌────▼────┐
                      │  CPU 0  │        │  CPU 1  │
                      └─────────┘        └─────────┘
```

### 5.2 中断类型

| 类型 | INTID 范围 | 说明 | 管理者 |
|------|-----------|------|--------|
| SGI (Software Generated) | 0-15 | 核间中断（IPI） | GICR |
| PPI (Private Peripheral) | 16-31 | CPU 私有中断（如定时器） | GICR |
| SPI (Shared Peripheral) | 32-1019 | 共享外设中断（如 UART） | GICD |
| Special | 1020-1023 | 特殊 INTID（1023=spurious） | - |

UART 中断是 SPI 类型，INTID ≥ 32。

### 5.3 对比 PLIC

```
PLIC                                GICv3
────────────────────────────────    ────────────────────────────────
单一 MMIO 区域                       GICD(MMIO) + GICR(MMIO) + ICC(sysreg)
priority[irq] 寄存器                 GICD_IPRIORITYR / GICR_IPRIORITYR0
enable[context] 位图                 GICD_ISENABLER / GICR_ISENABLER0
threshold[context]                   ICC_PMR_EL1 (优先级掩码)
claim = 读 claim 寄存器              claim = 读 ICC_IAR1_EL1
complete = 写 complete 寄存器        EOI = 写 ICC_EOIR1_EL1
```

---

## 6. RK3588 GIC 地址映射

来自 `rk3588s.dtsi:1574`：

```dts
gic: interrupt-controller@fe600000 {
    compatible = "arm,gic-v3";
    reg = <0x0 0xfe600000 0 0x10000>,   /* GICD — Distributor */
          <0x0 0xfe680000 0 0x100000>;  /* GICR — Redistributor */
    interrupts = <GIC_PPI 9 IRQ_TYPE_LEVEL_HIGH 0>;
};
```

### 6.1 地址总结

| 组件 | 基地址 | 大小 | 说明 |
|------|--------|------|------|
| GICD | `0xfe600000` | 64KB (0x10000) | Distributor，全局唯一 |
| GICR | `0xfe680000` | 1MB (0x100000) | Redistributor 区域 |
| GICR[cpu0] RD_base | `0xfe680000` | 64KB | CPU0 的 Redistributor |
| GICR[cpu0] SGI_base | `0xfe690000` | 64KB | CPU0 的 SGI/PPI 寄存器 |
| GICR[cpu1] RD_base | `0xfe6a0000` | 64KB | CPU1 的 Redistributor |
| GICR[cpu1] SGI_base | `0xfe6b0000` | 64KB | CPU1 的 SGI/PPI 寄存器 |
| ... | 每个 CPU +0x20000 | | RK3588 有 8 核 |

### 6.2 GICR 布局详解

每个 CPU 的 Redistributor 占 128KB（两个 64KB frame）：

```
GICR_base + cpu * 0x20000 + 0x00000  →  RD_base  (GICR_CTLR, GICR_TYPER, GICR_WAKER...)
GICR_base + cpu * 0x20000 + 0x10000  →  SGI_base (GICR_IGROUPR0, GICR_ISENABLER0, GICR_IPRIORITYR0...)
```

### 6.3 Rust 常量定义

```rust
// RK3588 GIC 基地址
const GICD_BASE: usize = 0xfe60_0000;
const GICR_BASE: usize = 0xfe68_0000;
const GICR_STRIDE: usize = 0x20000;  // 每个 CPU 的 GICR 间距

// 当前 CPU 的 GICR（单核先用 CPU0）
fn gicr_rd_base(cpu: usize) -> usize { GICR_BASE + cpu * GICR_STRIDE }
fn gicr_sgi_base(cpu: usize) -> usize { GICR_BASE + cpu * GICR_STRIDE + 0x10000 }
```

---

## 7. Distributor (GICD) 初始化

Distributor 管理所有 SPI 中断（INTID 32-1019），全局唯一。

### 7.1 寄存器偏移表

来自 `include/linux/irqchip/arm-gic-v3.h`：

```
偏移          名称              说明
──────────────────────────────────────────────────────
0x0000        GICD_CTLR         控制寄存器
0x0004        GICD_TYPER        类型寄存器（只读，查询中断数量）
0x0080        GICD_IGROUPR      中断分组（每bit一个中断）
0x0100        GICD_ISENABLER    中断使能设置（写1使能）
0x0180        GICD_ICENABLER    中断使能清除（写1禁用）
0x0200        GICD_ISPENDR      中断挂起设置
0x0280        GICD_ICPENDR      中断挂起清除
0x0400        GICD_IPRIORITYR   中断优先级（每中断8bit）
0x0C00        GICD_ICFGR        中断配置（边沿/电平）
0x6000        GICD_IROUTER      中断路由（每中断64bit，指定目标CPU）
```

### 7.2 GICD_CTLR 位域

```
bit 31    RWP     — 寄存器写入进行中（只读，轮询等待变0）
bit 8     nASSGI  — SGI 不需要 active 状态
bit 6     DS      — 单安全状态（只读）
bit 4     ARE_NS  — Affinity Routing Enable (Non-Secure) ★ 必须设1
bit 1     EnableGrp1A — 使能 Non-Secure Group 1 中断 ★ 必须设1
bit 0     EnableGrp1  — 使能 Secure Group 1 中断
```

### 7.3 初始化步骤（对照 Linux `gic_dist_init()`）

来自 `irq-gic-v3.c:914`：

```rust
fn gic_dist_init() {
    let base = GICD_BASE;

    // 1. 关闭 Distributor
    write32(base + GICD_CTLR, 0);
    gic_dist_wait_for_rwp();  // 等待 RWP 位清零

    // 2. 读取支持的中断数量
    let typer = read32(base + GICD_TYPER);
    let it_lines = ((typer & 0x1f) + 1) * 32;  // 最大 INTID

    // 3. 配置所有 SPI 为 Non-Secure Group 1
    //    IGROUPR: 每 bit 对应一个中断，1 = Group1
    //    SPI 从 INTID 32 开始，所以从 IGROUPR[1] 开始
    for i in (32..it_lines).step_by(32) {
        write32(base + GICD_IGROUPR + (i / 8) as usize, 0xFFFF_FFFF);
    }

    // 4. 禁用所有 SPI
    for i in (32..it_lines).step_by(32) {
        write32(base + GICD_ICENABLER + (i / 8) as usize, 0xFFFF_FFFF);
    }

    // 5. 设置所有 SPI 优先级为默认值 (0xa0)
    for i in (32..it_lines).step_by(4) {
        write32(base + GICD_IPRIORITYR + i as usize, 0xa0a0a0a0);
    }

    // 6. 设置所有 SPI 为电平触发
    for i in (32..it_lines).step_by(16) {
        write32(base + GICD_ICFGR + (i / 4) as usize, 0);
    }

    // 7. 设置所有 SPI 路由到 CPU0
    //    IROUTER: 64bit，写 Affinity 值
    let affinity = read_mpidr_affinity();  // 当前 CPU 的 affinity
    for i in 32..it_lines {
        write64(base + GICD_IROUTER + (i * 8) as usize, affinity);
    }

    // 8. 启用 Distributor
    //    ARE_NS | EnableGrp1A | EnableGrp1
    write32(base + GICD_CTLR, (1 << 4) | (1 << 1) | (1 << 0));
    gic_dist_wait_for_rwp();
}

fn gic_dist_wait_for_rwp() {
    // 轮询 GICD_CTLR.RWP (bit 31) 直到为 0
    while read32(GICD_BASE + GICD_CTLR) & (1 << 31) != 0 {
        core::hint::spin_loop();
    }
}
```

### 7.4 使能特定 SPI 中断

```rust
/// 使能一个 SPI 中断（INTID >= 32）
fn gic_enable_spi(intid: u32) {
    let reg = GICD_BASE + GICD_ISENABLER + ((intid / 32) * 4) as usize;
    let bit = 1u32 << (intid % 32);
    write32(reg, bit);  // 写1使能，其他位不受影响
}

/// 设置 SPI 优先级
fn gic_set_spi_priority(intid: u32, priority: u8) {
    let reg = GICD_BASE + GICD_IPRIORITYR + intid as usize;
    write8(reg, priority);
}
```

---

## 8. Redistributor (GICR) 初始化

Redistributor 是 per-CPU 的，管理 SGI (0-15) 和 PPI (16-31)。
定时器中断是 PPI，所以也归 GICR 管。

### 8.1 寄存器偏移表

RD_base 帧（偏移 +0x0000）：

```
偏移          名称              说明
──────────────────────────────────────────────────────
0x0000        GICR_CTLR         控制寄存器
0x0008        GICR_TYPER        类型寄存器（64bit，含 Affinity）
0x0014        GICR_WAKER        唤醒寄存器 ★ 初始化必须操作
```

SGI_base 帧（偏移 +0x10000）：

```
偏移          名称                说明
──────────────────────────────────────────────────────
0x0080        GICR_IGROUPR0      SGI/PPI 分组（同 GICD_IGROUPR）
0x0100        GICR_ISENABLER0    SGI/PPI 使能设置
0x0180        GICR_ICENABLER0    SGI/PPI 使能清除
0x0400        GICR_IPRIORITYR0   SGI/PPI 优先级
0x0C00        GICR_ICFGR0        SGI/PPI 配置
```

### 8.2 GICR_WAKER — 唤醒 Redistributor

Redistributor 上电后可能处于睡眠状态，必须先唤醒：

```
bit 1  ProcessorSleep  — 1=睡眠, 0=活跃。写0唤醒。
bit 2  ChildrenAsleep  — 1=子组件睡眠（只读）。等待变0。
```

### 8.3 初始化步骤（对照 Linux `gic_cpu_init()` + `gic_enable_redist()`）

来自 `irq-gic-v3.c:1270`：

```rust
fn gic_redist_init(cpu: usize) {
    let rd_base = gicr_rd_base(cpu);
    let sgi_base = gicr_sgi_base(cpu);

    // 1. 唤醒 Redistributor
    let mut waker = read32(rd_base + GICR_WAKER);
    waker &= !(1 << 1);  // 清除 ProcessorSleep
    write32(rd_base + GICR_WAKER, waker);

    // 等待 ChildrenAsleep 变为 0
    while read32(rd_base + GICR_WAKER) & (1 << 2) != 0 {
        core::hint::spin_loop();
    }

    // 2. 配置所有 SGI/PPI 为 Non-Secure Group 1
    write32(sgi_base + GICR_IGROUPR0, 0xFFFF_FFFF);

    // 3. 设置 SGI/PPI 默认优先级
    for i in (0..32).step_by(4) {
        write32(sgi_base + GICR_IPRIORITYR0 + i, 0xa0a0a0a0);
    }

    // 4. 禁用所有 SGI/PPI（后面按需开启）
    write32(sgi_base + GICR_ICENABLER0, 0xFFFF_FFFF);

    // 5. 等待写入完成
    gic_redist_wait_for_rwp(rd_base);
}

fn gic_redist_wait_for_rwp(rd_base: usize) {
    // GICR_CTLR.RWP = bit 3
    while read32(rd_base + GICR_CTLR) & (1 << 3) != 0 {
        core::hint::spin_loop();
    }
}
```

### 8.4 使能 PPI 中断（如定时器）

```rust
/// 使能一个 PPI 中断（INTID 16-31）
fn gic_enable_ppi(cpu: usize, intid: u32) {
    let sgi_base = gicr_sgi_base(cpu);
    let bit = 1u32 << intid;
    write32(sgi_base + GICR_ISENABLER0, bit);
}

// 例：使能 EL1 物理定时器中断 (PPI 30, INTID=30)
gic_enable_ppi(0, 30);
```

---

## 9. CPU Interface (ICC) 初始化

GICv3 的 CPU Interface 通过系统寄存器访问（不是 MMIO！），这是和 PLIC 最大的区别。

### 9.1 ICC 系统寄存器表

| 寄存器 | 编码 | 用途 |
|--------|------|------|
| `ICC_SRE_EL1` | `S3_0_C12_C12_5` | System Register Enable ★ |
| `ICC_PMR_EL1` | `S3_0_C4_C6_0` | Priority Mask（优先级阈值） |
| `ICC_BPR1_EL1` | `S3_0_C12_C12_3` | Binary Point（优先级分组） |
| `ICC_CTLR_EL1` | `S3_0_C12_C12_4` | 控制寄存器（EOI 模式） |
| `ICC_IGRPEN1_EL1` | `S3_0_C12_C12_7` | Group 1 中断使能 ★ |
| `ICC_IAR1_EL1` | `S3_0_C12_C12_0` | Interrupt Acknowledge（读=claim）★ |
| `ICC_EOIR1_EL1` | `S3_0_C12_C12_1` | End of Interrupt（写=complete）★ |
| `ICC_DIR_EL1` | `S3_0_C12_C11_1` | Deactivate Interrupt |

### 9.2 Rust 中访问 ICC 寄存器

```rust
/// 读 ICC_IAR1_EL1 — 获取当前最高优先级的 pending 中断号
#[inline]
fn gic_read_iar() -> u32 {
    let irqnr: u64;
    unsafe {
        core::arch::asm!("mrs {}, S3_0_C12_C12_0", out(reg) irqnr);
        core::arch::asm!("dsb sy");  // Linux 也加了这个 barrier
    }
    irqnr as u32
}

/// 写 ICC_EOIR1_EL1 — 通知 GIC 中断处理完成
#[inline]
fn gic_write_eoir(irqnr: u32) {
    unsafe {
        core::arch::asm!("msr S3_0_C12_C12_1, {}", in(reg) irqnr as u64);
        core::arch::asm!("isb");
    }
}

/// 读 ICC_SRE_EL1
#[inline]
fn gic_read_sre() -> u32 {
    let val: u64;
    unsafe { core::arch::asm!("mrs {}, S3_0_C12_C12_5", out(reg) val); }
    val as u32
}

/// 写 ICC_SRE_EL1
#[inline]
fn gic_write_sre(val: u32) {
    unsafe {
        core::arch::asm!("msr S3_0_C12_C12_5, {}", in(reg) val as u64);
        core::arch::asm!("isb");
    }
}

/// 写 ICC_PMR_EL1 — 设置优先级掩码
#[inline]
fn gic_write_pmr(val: u32) {
    unsafe {
        core::arch::asm!("msr S3_0_C4_C6_0, {}", in(reg) val as u64);
    }
}

/// 写 ICC_IGRPEN1_EL1 — 使能 Group 1 中断
#[inline]
fn gic_write_grpen1(val: u32) {
    unsafe {
        core::arch::asm!("msr S3_0_C12_C12_7, {}", in(reg) val as u64);
        core::arch::asm!("isb");
    }
}

/// 写 ICC_BPR1_EL1
#[inline]
fn gic_write_bpr1(val: u32) {
    unsafe {
        core::arch::asm!("msr S3_0_C12_C12_3, {}", in(reg) val as u64);
    }
}

/// 写 ICC_CTLR_EL1
#[inline]
fn gic_write_ctlr(val: u32) {
    unsafe {
        core::arch::asm!("msr S3_0_C12_C12_4, {}", in(reg) val as u64);
        core::arch::asm!("isb");
    }
}
```

### 9.3 初始化步骤（对照 Linux `gic_cpu_sys_reg_init()`）

来自 `irq-gic-v3.c:1135`：

```rust
fn gic_cpu_interface_init() {
    // 1. 启用系统寄存器接口
    //    ICC_SRE_EL1.SRE = 1
    //    如果 EL2 没有设置 ICC_SRE_EL2.SRE，这里会失败
    let sre = gic_read_sre();
    if sre & 1 == 0 {
        gic_write_sre(sre | 1);
        let sre = gic_read_sre();
        assert!(sre & 1 != 0, "GIC: SRE disabled at EL2!");
    }

    // 2. 设置优先级掩码 — 允许所有优先级的中断
    //    0xFF = 最低优先级，所有中断都能通过
    gic_write_pmr(0xFF);

    // 3. 设置 BPR1 = 0（最细粒度的优先级分组）
    gic_write_bpr1(0);

    // 4. 设置 EOI 模式
    //    EOImode = 0: 写 EOIR 同时 deactivate（简单模式）
    gic_write_ctlr(0);  // EOImode_drop_dir

    // 5. 使能 Group 1 中断 ★ 这是最终开关
    //    等价于 RISC-V 的 sie::set_sext()
    gic_write_grpen1(1);
}
```

### 9.4 初始化顺序总结

```
gic_dist_init()           ← 全局一次
    ↓
gic_redist_init(cpu)      ← 每个 CPU 一次
    ↓
gic_cpu_interface_init()  ← 每个 CPU 一次
    ↓
gic_enable_spi(uart_irq)  ← 按需使能具体中断
    ↓
daifclr #2                ← 开 IRQ（在用户态通过 SPSR 自动恢复）
```

---

## 10. 中断处理流程

### 10.1 完整中断路径（设备 → 你的 handler）

```
UART 数据到达
    │
    ▼
UART 拉高 IRQ 线 (SPI, INTID=N)
    │
    ▼
GICD 检查：ISENABLER[N]=1? IGROUPR[N]=Group1? 优先级够高?
    │ 是
    ▼
GICD 通过 IROUTER[N] 找到目标 CPU
    │
    ▼
GICR[cpu] 转发到 CPU Interface
    │
    ▼
ICC 检查：IGRPEN1=1? PMR 允许? 优先级 > 当前运行优先级?
    │ 是
    ▼
CPU 检查：PSTATE.I = 0? (IRQ 未屏蔽)
    │ 是
    ▼
CPU 触发 IRQ 异常
    │
    ├── 如果在 EL0 → 跳转到 VBAR_EL1 + 0x480 (irq_el0_64)
    └── 如果在 EL1 → 跳转到 VBAR_EL1 + 0x280 (irq_el1_spx)
    │
    ▼
你的 handler:
    1. 保存寄存器
    2. mrs x0, S3_0_C12_C12_0    // 读 ICC_IAR1_EL1 → 获取 INTID (claim)
    3. 检查 INTID != 1023         // 1023 = spurious
    4. 根据 INTID 分发处理
    5. msr S3_0_C12_C12_1, x0    // 写 ICC_EOIR1_EL1 (EOI/complete)
    6. 恢复寄存器
    7. eret
```

### 10.2 Rust handler 实现

```rust
/// GIC 中断分发 — 在 irq_el0_64 和 irq_el1_spx 中调用
fn gic_handle_irq() {
    loop {
        let irqnr = gic_read_iar();

        // 1023 = spurious，没有更多 pending 中断
        if irqnr == 1023 {
            break;
        }

        // 分发处理
        match irqnr {
            30 => {
                // PPI 30 = EL1 Physical Timer
                set_next_timeInterupt();
            }
            N if N >= 32 => {
                // SPI — 外设中断
                if N == UART_IRQ {
                    keyboard_interrupt_handler();
                } else {
                    warn!("未知 SPI 中断: {}", N);
                }
            }
            _ => {
                warn!("未处理的中断: {}", irqnr);
            }
        }

        // EOI — 通知 GIC 处理完成
        gic_write_eoir(irqnr);
    }
}
```

### 10.3 关键：claim 和 EOI 必须配对

和 PLIC 的 claim/complete 一样，每次读 `ICC_IAR1_EL1` 必须对应一次写 `ICC_EOIR1_EL1`。
如果不写 EOI，GIC 认为中断还在处理中，不会再发送同优先级或更低优先级的中断。

### 10.4 Spurious 中断 (INTID 1023)

读 `ICC_IAR1_EL1` 返回 1023 表示没有 pending 中断（可能中断已被撤销）。
对 spurious 中断不要写 EOI。

来自 `irq-gic-v3.c:773`：
```c
static void __gic_handle_irq(u32 irqnr, struct pt_regs *regs)
{
    if (gic_irqnr_is_special(irqnr))  // 1020-1023
        return;                         // 不写 EOI
    gic_complete_ack(irqnr);           // 写 EOIR
    handle_domain_irq(gic_data.domain, irqnr);
}
```

---

## 11. 内核态中断处理

这是你在 RISC-V 上刚解决的问题，AArch64 上原理相同但实现更简单。

### 11.1 问题回顾

内核态执行时 `PSTATE.I = 1`（IRQ 屏蔽），设备中断无法触发。
需要在 `get_char()` 等待 I/O 时临时开 IRQ，让中断能进来。

### 11.2 AArch64 的优势

AArch64 的向量表天然区分了内核态和用户态的 IRQ 入口：
- `irq_el1_spx`（偏移 0x280）— 内核态 IRQ，用当前 SP（内核栈）
- `irq_el0_64`（偏移 0x480）— 用户态 IRQ，需要切换栈

不需要像 RISC-V 那样动态切换 stvec！向量表是固定的，硬件自动选择正确的入口。

### 11.3 irq_el1_spx 的汇编实现

内核态 IRQ 处理比用户态简单得多：不需要切换栈、不需要切换页表。

```asm
.align 7
irq_el1_spx:
    // 在当前内核栈上保存 caller-saved 寄存器
    sub sp, sp, #(18 * 8)       // 分配栈帧

    stp x0,  x1,  [sp, #(0  * 8)]
    stp x2,  x3,  [sp, #(2  * 8)]
    stp x4,  x5,  [sp, #(4  * 8)]
    stp x6,  x7,  [sp, #(6  * 8)]
    stp x8,  x9,  [sp, #(8  * 8)]
    stp x10, x11, [sp, #(10 * 8)]
    stp x12, x13, [sp, #(12 * 8)]
    stp x14, x15, [sp, #(14 * 8)]
    stp x29, x30, [sp, #(16 * 8)]  // FP + LR

    // 保存 ELR_EL1 和 SPSR_EL1（中断可能嵌套）
    // 注意：如果不打算嵌套中断，可以不保存，但保存更安全
    // 这里用 x29/x30 已经保存了，可以复用临时寄存器

    // 调用 Rust handler
    bl kernel_irq_handler

    // 恢复寄存器
    ldp x0,  x1,  [sp, #(0  * 8)]
    ldp x2,  x3,  [sp, #(2  * 8)]
    ldp x4,  x5,  [sp, #(4  * 8)]
    ldp x6,  x7,  [sp, #(6  * 8)]
    ldp x8,  x9,  [sp, #(8  * 8)]
    ldp x10, x11, [sp, #(10 * 8)]
    ldp x12, x13, [sp, #(12 * 8)]
    ldp x14, x15, [sp, #(14 * 8)]
    ldp x29, x30, [sp, #(16 * 8)]

    add sp, sp, #(18 * 8)
    eret
```

对应的 Rust handler：

```rust
#[no_mangle]
pub extern "C" fn kernel_irq_handler() {
    // 和用户态一样调用 gic_handle_irq()
    // 但不做任务调度，只处理中断本身
    gic_handle_irq();
}
```

### 11.4 对比 RISC-V 方案

```
RISC-V:                              AArch64:
─────────────────────────────────    ─────────────────────────────────
需要 set_kernel_trap() 切换 stvec    不需要！向量表固定，硬件自动选入口
kernel_trap.asm 手动保存寄存器       irq_el1_spx 入口保存寄存器
set_sie() + wfi + clear_sie()       daifclr #2 + wfi + daifset #2
SBI ecall 可能破坏 sepc              没有这个问题（timer 是 PPI，本地处理）
```

### 11.5 get_char 中的 wfi

```rust
pub fn get_char() -> u8 {
    loop {
        if let Some(c) = read_input() {
            return c;
        }
        // AArch64: 临时开 IRQ，等待中断唤醒
        unsafe {
            core::arch::asm!("msr daifclr, #2");  // 开 IRQ
            core::arch::asm!("wfi");               // 等待
            core::arch::asm!("msr daifset, #2");  // 关 IRQ
        }
    }
}
```

---

## 12. UART 中断接入

### 12.1 RK3588 UART 中断号

RK3588 有多个 UART，从设备树中可以找到中断号。
设备树中的中断号格式：`<GIC_SPI N IRQ_TYPE_LEVEL_HIGH>`

其中 `GIC_SPI N` 表示 SPI 中断，实际 INTID = N + 32。

例如 UART2（通常是调试串口）：
```dts
serial2: serial@feb50000 {
    compatible = "rockchip,rk3588-uart", "snps,dw-apb-uart";
    reg = <0x0 0xfeb50000 0x0 0x100>;
    interrupts = <GIC_SPI 364 IRQ_TYPE_LEVEL_HIGH>;
    ...
};
```

INTID = 364 + 32 = 396（注意：具体值需要查你用的 UART 的设备树节点）

### 12.2 使能 UART 中断的完整流程

```rust
const UART_SPI_ID: u32 = 364;  // 设备树中的 SPI 号（需要根据实际 UART 确认）
const UART_INTID: u32 = UART_SPI_ID + 32;  // GIC INTID

fn enable_uart_interrupt() {
    // 1. 在 GIC 中使能这个 SPI
    gic_enable_spi(UART_INTID);

    // 2. 设置优先级（可选，默认 0xa0 已经够用）
    gic_set_spi_priority(UART_INTID, 0xa0);

    // 3. 在 UART 硬件中使能 RX 中断
    //    16550 UART: IER bit 0 = RX Data Available
    let ier_addr = UART_BASE + 1;  // IER 偏移
    unsafe {
        (ier_addr as *mut u8).write_volatile(0x01);
    }
}
```

### 12.3 在 IRQ handler 中处理 UART

```rust
fn gic_handle_irq() {
    loop {
        let irqnr = gic_read_iar();
        if irqnr == 1023 { break; }

        match irqnr {
            30 => {
                // Timer PPI
                set_next_timeInterupt();
            }
            UART_INTID => {
                // UART 中断 — 复用你已有的 keyboard_interrupt_handler
                keyboard_interrupt_handler();
            }
            _ => {}
        }

        gic_write_eoir(irqnr);
    }
}
```

---

## 13. 完整初始化伪代码

把所有步骤串起来，这是你需要在 `main.rs` 中调用的初始化序列：

```rust
// ============ 启动时调用 ============

pub fn init_gic_and_interrupts() {
    // 第一步：初始化 Distributor（全局一次）
    gic_dist_init();

    // 第二步：初始化当前 CPU 的 Redistributor
    gic_redist_init(0);  // CPU 0

    // 第三步：初始化 CPU Interface（系统寄存器）
    gic_cpu_interface_init();

    // 第四步：使能具体的中断源
    gic_enable_spi(UART_INTID);     // UART
    // gic_enable_ppi(0, 30);        // Timer PPI（如果需要 GIC 管理）

    // 第五步：UART 硬件使能 RX 中断
    enable_uart_rx_interrupt();

    // 注意：不要在这里 daifclr！
    // IRQ 在用户态通过 SPSR_EL1 → PSTATE 自动恢复
    // 内核态只在 get_char 的 wfi 前临时开启
}
```

### 13.1 对比 RISC-V 的初始化

```
RISC-V:                              AArch64:
─────────────────────────────────    ─────────────────────────────────
plic_init()                          gic_dist_init() + gic_redist_init()
  - 设置优先级                         - 设置 IGROUPR, 优先级, 路由
  - 设置使能位                         - 唤醒 GICR
  - 设置阈值                           - gic_cpu_interface_init()
                                       - 设置 SRE, PMR, GRPEN1
enable_uart_rx_interrupt()           enable_uart_rx_interrupt()
  - UART IER = 0x01                    - UART IER = 0x01
enable_external_interrupt()          (已包含在 gic_cpu_interface_init)
  - sie::set_sext()                    - gic_write_grpen1(1)
```

---

## 14. Linux 源码参考索引

你的 Linux 源码在 `other/linux-5.4.29/` 和 `WSL2-Linux-Kernel/`。

### 14.1 核心文件

| 文件 | 内容 |
|------|------|
| `WSL2-Linux-Kernel/drivers/irqchip/irq-gic-v3.c` | GICv3 驱动主文件 |
| `WSL2-Linux-Kernel/include/linux/irqchip/arm-gic-v3.h` | 所有 GICD/GICR 寄存器偏移定义 |
| `WSL2-Linux-Kernel/arch/arm64/include/asm/arch_gicv3.h` | ICC 系统寄存器访问函数 |
| `WSL2-Linux-Kernel/arch/arm64/include/asm/sysreg.h` | 系统寄存器编码 |
| `WSL2-Linux-Kernel/arch/arm64/include/asm/daifflags.h` | DAIF 操作函数 |
| `WSL2-Linux-Kernel/arch/arm64/kernel/entry.S` | 异常向量表 |
| `WSL2-Linux-Kernel/arch/arm64/kernel/entry-common.c` | 异常处理 C 入口 |
| `WSL2-Linux-Kernel/arch/arm64/boot/dts/rockchip/rk3588s.dtsi` | RK3588 GIC 地址 |

### 14.2 关键函数对照

| Linux 函数 | 行号 | 你需要实现的对应功能 |
|-----------|------|---------------------|
| `gic_dist_init()` | irq-gic-v3.c:914 | Distributor 初始化 |
| `gic_enable_redist()` | 唤醒 GICR | Redistributor 唤醒 |
| `gic_cpu_init()` | irq-gic-v3.c:1270 | GICR SGI/PPI 配置 |
| `gic_cpu_sys_reg_init()` | irq-gic-v3.c:1135 | ICC 系统寄存器初始化 |
| `gic_handle_irq()` | irq-gic-v3.c:868 | 中断顶层 handler |
| `__gic_handle_irq()` | irq-gic-v3.c:771 | claim → dispatch → EOI |
| `gic_read_iar_common()` | arch_gicv3.h:36 | 读 ICC_IAR1_EL1 |
| `gic_eoi_irq()` | irq-gic-v3.c:623 | 写 ICC_EOIR1_EL1 |
| `gic_enable_sre()` | arm-gic-v3.h:644 | 启用系统寄存器接口 |

### 14.3 另一个参考：StarryOS

`othersrc/StarryOS/arceos/modules/axdriver/src/dyn_drivers/intc/gicv3.rs`
这是一个 Rust 写的 GICv3 驱动，可以参考其 MMIO 读写封装。

---

## 附录 A：MMIO 读写辅助函数

```rust
#[inline]
fn write32(addr: usize, val: u32) {
    unsafe { (addr as *mut u32).write_volatile(val); }
}

#[inline]
fn read32(addr: usize) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

#[inline]
fn write64(addr: usize, val: u64) {
    unsafe { (addr as *mut u64).write_volatile(val); }
}

#[inline]
fn write8(addr: usize, val: u8) {
    unsafe { (addr as *mut u8).write_volatile(val); }
}

/// 读取当前 CPU 的 MPIDR affinity
fn read_mpidr_affinity() -> u64 {
    let mpidr: u64;
    unsafe { core::arch::asm!("mrs {}, mpidr_el1", out(reg) mpidr); }
    // 提取 Aff3:Aff2:Aff1:Aff0
    let aff = ((mpidr >> 32) & 0xFF) << 32
            | ((mpidr >> 16) & 0xFF) << 16
            | ((mpidr >> 8) & 0xFF) << 8
            | (mpidr & 0xFF);
    aff
}
```

## 附录 B：完整中断流程 ASCII 图

```
┌──────────┐     IRQ线      ┌──────────────────────────────────────┐
│  UART    │ ──────────────→│  GICD (Distributor)                  │
│  16550   │                │  0xfe600000                          │
│          │                │                                      │
│ IER=0x01 │                │  ISENABLER[N/32] bit N%32 = 1?      │
│ (RX中断) │                │  IGROUPR[N/32] bit N%32 = 1? (Grp1) │
│          │                │  IPRIORITYR[N] < PMR?                │
└──────────┘                │  IROUTER[N] → CPU0                   │
                            └──────────────┬───────────────────────┘
                                           │
                            ┌──────────────▼───────────────────────┐
                            │  GICR (Redistributor, CPU0)          │
                            │  0xfe680000                          │
                            │  转发 SPI 到 CPU Interface            │
                            └──────────────┬───────────────────────┘
                                           │
                            ┌──────────────▼───────────────────────┐
                            │  ICC (CPU Interface, 系统寄存器)      │
                            │                                      │
                            │  IGRPEN1 = 1?  (Group1 使能)         │
                            │  PMR > 中断优先级?                    │
                            │  → 向 CPU 发出 IRQ 信号               │
                            └──────────────┬───────────────────────┘
                                           │
                            ┌──────────────▼───────────────────────┐
                            │  CPU Core                            │
                            │                                      │
                            │  PSTATE.I = 0? (IRQ 未屏蔽)          │
                            │  → 触发 IRQ 异常                     │
                            │  → 跳转到 VBAR_EL1 + offset          │
                            └──────────────┬───────────────────────┘
                                           │
                    ┌──────────────────────┬┴──────────────────────┐
                    │ 来自 EL0 (用户态)     │ 来自 EL1 (内核态)     │
                    │ offset = 0x480       │ offset = 0x280        │
                    │ irq_el0_64           │ irq_el1_spx           │
                    │                      │                       │
                    │ 1. 保存全部寄存器     │ 1. 保存 caller-saved  │
                    │ 2. 切换到内核栈/页表  │ 2. (已在内核栈)       │
                    │ 3. gic_handle_irq()  │ 3. gic_handle_irq()  │
                    │ 4. 调度/返回用户态    │ 4. 恢复寄存器         │
                    │                      │ 5. eret               │
                    └──────────────────────┴───────────────────────┘
                                           │
                            ┌──────────────▼───────────────────────┐
                            │  gic_handle_irq()                    │
                            │                                      │
                            │  irqnr = ICC_IAR1_EL1  (claim)      │
                            │  if irqnr == 1023: return (spurious) │
                            │  dispatch(irqnr)                     │
                            │  ICC_EOIR1_EL1 = irqnr  (EOI)       │
                            └──────────────────────────────────────┘
```
