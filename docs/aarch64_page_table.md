# AArch64 页表机制详解

> 基于 Linux 5.4.29 源码 (`arch/arm64/`) 和 ARM Architecture Reference Manual

---

## 目录

1. [PTE (Page Table Entry) 结构位图](#1-pte-page-table-entry-结构位图)
2. [关键系统寄存器](#2-关键系统寄存器)
3. [页表遍历过程](#3-页表遍历过程)
4. [从物理地址划分到开启页表的完整流程](#4-从物理地址划分到开启页表的完整流程)

---

## 1. PTE (Page Table Entry) 结构位图

### 1.1 Level 3 PTE (页描述符) - 64位

来源: `arch/arm64/include/asm/pgtable-hwdef.h`

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Bit  63    60-59  58   57   56   55   54   53   52   51   50-48  47-12     │
│     ┌───┬─────┬────┬────┬────┬────┬────┬────┬────┬────┬─────┬─────────────┐ │
│     │SW │RES0 │PN  │DEVM│SPEC│DIRT│UXN │PXN │CONT│DBM │RES0 │物理地址[47:12]│ │
│     └───┴─────┴────┴────┴────┴────┴────┴────┴────┴────┴─────┴─────────────┘ │
├─────────────────────────────────────────────────────────────────────────────┤
│ Bit  11   10   9-8    7      6      5-4    3-2     1      0                │
│     ┌────┬────┬─────┬──────┬──────┬─────┬──────┬──────┬──────┐              │
│     │ nG │ AF │ SH[1:0] │AP[2]│AP[1]│RES0 │AttrIdx│TABLE│VALID│              │
│     └────┴────┴─────┴──────┴──────┴─────┴──────┴──────┴──────┘              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 各位域详解

| 位域 | 名称 | 说明 | Linux定义 |
|------|------|------|-----------|
| **0** | VALID | 有效位，1=条目有效 | `PTE_VALID` |
| **1** | TABLE/TYPE | 类型位，与bit[0]组成类型 | `PTE_TABLE_BIT` |
| **[3:2]** | AttrIndx | 内存属性索引，指向MAIR_EL1 | `PTE_ATTRINDX(t)` |
| **[5:4]** | RES0 | 保留为0 | - |
| **6** | AP[1] | 访问权限：1=EL0可访问 | `PTE_USER` |
| **7** | AP[2] | 访问权限：1=只读 | `PTE_RDONLY` |
| **[9:8]** | SH | 共享属性：00=不共享, 11=内部共享 | `PTE_SHARED` |
| **10** | AF | Access Flag，访问标志 | `PTE_AF` |
| **11** | nG | non-Global，非全局(ASID相关) | `PTE_NG` |
| **[47:12]** | Output Address | 物理地址[47:12] (4K对齐) | `PTE_ADDR_MASK` |
| **51** | DBM | Dirty Bit Management | `PTE_DBM` / `PTE_WRITE` |
| **52** | CONT | Contiguous bit，连续映射 | `PTE_CONT` |
| **53** | PXN | Privileged Execute-Never | `PTE_PXN` |
| **54** | UXN | User Execute-Never | `PTE_UXN` |
| **55** | DIRTY | 软件脏位 (Linux定义) | `PTE_DIRTY` |
| **56** | SPECIAL | 特殊页 (Linux定义) | `PTE_SPECIAL` |
| **57** | DEVMAP | 设备映射 (Linux定义) | `PTE_DEVMAP` |
| **58** | PROT_NONE | 无权限 (Linux定义) | `PTE_PROT_NONE` |

### 1.3 类型编码 (bit[1:0])

```
┌─────────────────────────────────────┐
│  bit[1:0]  │  类型                  │
├─────────────────────────────────────┤
│    00      │  无效 (Invalid)        │
│    01      │  Block描述符           │
│    11      │  Table/Page描述符      │
└─────────────────────────────────────┘
```

来源: `pgtable-hwdef.h`
```c
#define PTE_VALID       (_AT(pteval_t, 1) << 0)
#define PTE_TYPE_MASK   (_AT(pteval_t, 3) << 0)
#define PTE_TYPE_PAGE   (_AT(pteval_t, 3) << 0)
#define PTE_TABLE_BIT   (_AT(pteval_t, 1) << 1)
```

### 1.4 共享属性 SH[1:0]

```
┌───────────────────────────────────────────────────────┐
│  SH[1:0]  │  含义          │  说明                    │
├───────────────────────────────────────────────────────┤
│    00     │  Non-shareable │  不共享                  │
│    01     │  Reserved      │  保留                    │
│    10     │  Outer Shareable│ 外部共享                │
│    11     │  Inner Shareable│ 内部共享 (常用)         │
└───────────────────────────────────────────────────────┘
```

### 1.5 访问权限 AP[2:1]

```
┌───────────────────────────────────────────────────────────────┐
│  AP[2]  │  AP[1]  │  EL0访问  │  EL1访问  │  说明            │
├───────────────────────────────────────────────────────────────┤
│    0    │    0    │    否     │   读/写   │  内核RW          │
│    0    │    1    │    是     │   读/写   │  用户RW, 内核RW  │
│    1    │    0    │    否     │   只读    │  内核RO          │
│    1    │    1    │    是     │   只读    │  用户RO, 内核RO  │
└───────────────────────────────────────────────────────────────┘
```

### 1.6 AttrIndx[2:0] 与 MAIR_EL1 的关系

来源: `arch/arm64/mm/proc.S`

```
MAIR_EL1 寄存器 (Memory Attribute Indirection Register)
┌─────────────────────────────────────────────────────────────────────────────┐
│  Attr7  │  Attr6  │  Attr5  │  Attr4  │  Attr3  │  Attr2  │  Attr1  │  Attr0 │
│ [63:56] │ [55:48] │ [47:40] │ [39:32] │ [31:24] │ [23:16] │ [15:8]  │ [7:0]  │
└─────────────────────────────────────────────────────────────────────────────┘
         │         │         │         │         │         │         │
         │         │         │         │         │         │         └─ AttrIdx=0: Device-nGnRnE
         │         │         │         │         │         └─────────── AttrIdx=1: Device-nGnRE
         │         │         │         │         └───────────────────── AttrIdx=2: Device-GRE
         │         │         │         └─────────────────────────────── AttrIdx=3: Normal-NC
         │         │         └───────────────────────────────────────── AttrIdx=4: Normal (WB)
         │         └─────────────────────────────────────────────────── AttrIdx=5: Normal-WT
         └───────────────────────────────────────────────────────────── AttrIdx=6: (reserved)
```

Linux MAIR_EL1 初始化值 (来自 `proc.S`):
```asm
ldr x5, =MAIR(0x00, MT_DEVICE_nGnRnE) | \   // AttrIdx=0: 0x00
            MAIR(0x04, MT_DEVICE_nGnRE) | \  // AttrIdx=1: 0x04
            MAIR(0x0c, MT_DEVICE_GRE) | \    // AttrIdx=2: 0x0c
            MAIR(0x44, MT_NORMAL_NC) | \     // AttrIdx=3: 0x44
            MAIR(0xff, MT_NORMAL) | \        // AttrIdx=4: 0xff
            MAIR(0xbb, MT_NORMAL_WT)         // AttrIdx=5: 0xbb
msr mair_el1, x5
```

### 1.7 Block描述符 (Level 1/2)

来源: `pgtable-hwdef.h`

```
PMD Section (Level 2 Block) - 2MB 映射:
┌─────────────────────────────────────────────────────────────────────────────┐
│ Bit  63    60-59  58   57   56   55   54   53   52   51   50-48  47-21     │
│     ┌───┬─────┬────┬────┬────┬────┬────┬────┬────┬────┬─────┬─────────────┐ │
│     │SW │RES0 │    │    │    │    │UXN │PXN │CONT│    │RES0 │物理地址[47:21]│ │
│     └───┴─────┴────┴────┴────┴────┴────┴────┴────┴────┴─────┴─────────────┘ │
├─────────────────────────────────────────────────────────────────────────────┤
│ Bit  11   10   9-8    7      6      5-4    3-2     1      0                │
│     ┌────┬────┬─────┬──────┬──────┬─────┬──────┬──────┬──────┐              │
│     │ nG │ AF │ SH[1:0] │AP[2]│AP[1]│RES0 │AttrIdx│ 0    │ 1    │              │
│     └────┴────┴─────┴──────┴──────┴─────┴──────┴──────┴──────┘              │
└─────────────────────────────────────────────────────────────────────────────┘

关键定义:
#define PMD_TYPE_SECT   (_AT(pmdval_t, 1) << 0)   // Block描述符
#define PMD_TYPE_TABLE  (_AT(pmdval_t, 3) << 0)   // Table描述符
#define PMD_SECT_VALID  (_AT(pmdval_t, 1) << 0)   // 有效
#define PMD_SECT_AF     (_AT(pmdval_t, 1) << 10)  // Access Flag
#define PMD_SECT_USER   (_AT(pmdval_t, 1) << 6)   // AP[1]
#define PMD_SECT_RDONLY (_AT(pmdval_t, 1) << 7)   // AP[2]
```

---

## 2. 关键系统寄存器

### 2.1 TTBR0_EL1 / TTBR1_EL1 (Translation Table Base Register)

```
TTBR0_EL1 (用户空间页表基址, 低地址空间 VA[63] = 0)
TTBR1_EL1 (内核空间页表基址, 高地址空间 VA[63] = 1)

┌─────────────────────────────────────────────────────────────────────────────┐
│  64    63-52     51-48    47-2              1      0                       │
│ ┌────┬────────┬────────┬────────────────────┬────────────┬─────┐           │
│ │RES0│ ASID   │  RES0  │ BADDR[47:2]        │   RES0     │ CNP │           │
│ └────┴────────┴────────┴────────────────────┴────────────┴─────┘           │
│        16位ASID        页表基址(4K对齐)                    CNP位            │
└─────────────────────────────────────────────────────────────────────────────┘

ASID (Address Space Identifier): 16位，用于TLB标签
BADDR: 页表基址物理地址
CNP: Common not Private (ARMv8.2-A)
```

来源: `arch/arm64/mm/proc.S`
```asm
ENTRY(cpu_do_switch_mm)
    mrs     x2, ttbr1_el1
    mmid    x1, x1              // get mm->context.id
    phys_to_ttbr x3, x0
    bfi     x2, x1, #48, #16    // set the ASID in TTBR1
    msr     ttbr1_el1, x2
    isb
    msr     ttbr0_el1, x3       // update TTBR0
    isb
```

### 2.2 TCR_EL1 (Translation Control Register)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  63-59  58   57-55  54   53   52   51-48  47-44  43-40  39   38   37   36   │
│ ┌─────┬────┬─────┬────┬────┬────┬─────┬─────┬─────┬────┬────┬────┬────┐    │
│ │ IPS │TBI1│ASID │TBI0 │NFD1│NFD0│ HA  │  0  │ HD  │TCF0│TCF1│TBI0│ASID│    │
│ │     │    │SIZE │    │    │    │     │     │     │    │    │    │16  │    │
│ └─────┴────┴─────┴────┴────┴────┴─────┴─────┴─────┴────┴────┴────┴────┘    │
├─────────────────────────────────────────────────────────────────────────────┤
│  35-32  31   30-29  28   27   26-25  24-23  22   21-20  19-16               │
│ ┌─────┬────┬─────┬────┬────┬─────┬─────┬────┬─────┬─────┐                   │
│ │  0  │  0 │TG1  │SH1 │ ORGN1│IRGN1│  0  │EPD1│ A1  │ T1SZ │                   │
│ └─────┴────┴─────┴────┴────┴─────┴─────┴────┴─────┴─────┘                   │
├─────────────────────────────────────────────────────────────────────────────┤
│  15-14  13   12-11  10   9    8-7   6-5    4     3-2    1     0            │
│ ┌─────┬────┬─────┬────┬────┬────┬─────┬────┬─────┬────┬────┐               │
│ │TG0  │  0 │SH0  │ORGN0│IRGN0│  0  │EPD0 │ 0  │ T0SZ │  0 │  0 │               │
│ └─────┴────┴─────┴────┴────┴────┴─────┴────┴─────┴────┴────┘               │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 关键位域详解

| 位域 | 名称 | 说明 |
|------|------|------|
| **[5:0]** | T0SZ | TTBR0 地址空间大小 = 64 - T0SZ |
| **[15:14]** | TG0 | TTBR0粒度: 00=4KB, 01=64KB, 10=16KB |
| **[8:7]** | IRGN0 | TTBR0内部缓存属性: 01=WBWA |
| **[10:9]** | ORGN0 | TTBR0外部缓存属性: 01=WBWA |
| **[12:11]** | SH0 | TTBR0共享属性: 11=Inner Shareable |
| **[21:16]** | T1SZ | TTBR1 地址空间大小 = 64 - T1SZ |
| **[30:29]** | TG1 | TTBR1粒度: 10=4KB, 11=64KB, 01=16KB |
| **[23]** | EPD1 | TTBR1禁用: 1=禁用TTBR1 |
| **[22]** | A1 | ASID选择: 1=TTBR1.ASID |
| **[32:34]** | IPS | 中间物理地址大小 |

来源: `arch/arm64/include/asm/pgtable-hwdef.h`
```c
#define TCR_T0SZ(x)     ((UL(64) - (x)) << TCR_T0SZ_OFFSET)
#define TCR_T1SZ(x)     ((UL(64) - (x)) << TCR_T1SZ_OFFSET)
#define TCR_TG0_4K      (UL(0) << TCR_TG0_SHIFT)
#define TCR_TG0_16K     (UL(2) << TCR_TG0_SHIFT)
#define TCR_TG0_64K     (UL(1) << TCR_TG0_SHIFT)
#define TCR_IRGN0_WBWA  (UL(1) << TCR_IRGN0_SHIFT)
#define TCR_ORGN0_WBWA  (UL(1) << TCR_ORGN0_SHIFT)
#define TCR_SH0_INNER   (UL(3) << TCR_SH0_SHIFT)
#define TCR_A1          (UL(1) << 22)
#define TCR_ASID16      (UL(1) << 36)
```

### 2.3 SCTLR_EL1 (System Control Register)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  63-44 ... │ 31   30   29   28 ... 19   18   17 ... 13  12  11 ... 3  2  1  0 │
│ ─────────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬─────┬────┬────┬────┬────┐
│          │ENIA │ENIB │ ... │EE   │ ... │WXN  │ ... │ENDB │  I  │ ...│  C │  A │  M │
│          └─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴─────┴────┴────┴────┴────┘
└─────────────────────────────────────────────────────────────────────────────┘

关键位:
  M (bit 0):  MMU使能
  A (bit 1):  对齐检查使能
  C (bit 2):  数据缓存使能
  I (bit 12): 指令缓存使能
  WXN (bit 19): Write implies XN
  EE (bit 25): 大端模式
```

来源: `arch/arm64/include/asm/sysreg.h`
```c
#define SCTLR_ELx_M     (BIT(0))   // MMU使能
#define SCTLR_ELx_A     (BIT(1))   // 对齐检查
#define SCTLR_ELx_C     (BIT(2))   // 数据缓存
#define SCTLR_ELx_I     (BIT(12))  // 指令缓存
#define SCTLR_ELx_WXN   (BIT(19))  // Write implies XN
#define SCTLR_ELx_EE    (BIT(25))  // Endianness
```

### 2.4 MAIR_EL1 (Memory Attribute Indirection Register)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  Attr7   Attr6   Attr5   Attr4   Attr3   Attr2   Attr1   Attr0            │
│ [63:56] [55:48] [47:40] [39:32] [31:24] [23:16] [15:8]  [7:0]              │
└─────────────────────────────────────────────────────────────────────────────┘

Linux内存类型定义 (来自 proc.S):
  Attr0 (0x00): MT_DEVICE_nGnRnE - Device non-Gathering, non-Reordering, no Early write
  Attr1 (0x04): MT_DEVICE_nGnRE  - Device non-Gathering, non-Reordering, Early write
  Attr2 (0x0c): MT_DEVICE_GRE    - Device Gathering, Reordering, Early write
  Attr3 (0x44): MT_NORMAL_NC     - Normal Memory, Non-Cacheable
  Attr4 (0xff): MT_NORMAL        - Normal Memory, Write-Back Cacheable
  Attr5 (0xbb): MT_NORMAL_WT     - Normal Memory, Write-Through Cacheable
```

---

## 3. 页表遍历过程

### 3.1 虚拟地址分解 (以4KB页, 4级页表, 48位VA为例)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    64位虚拟地址 (VA)                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│  63   48    47    39    38    30    29    21    20    12    11     0       │
│ ┌────┬────┬────────┬────────┬────────┬────────┬────────┬────────┐          │
│ │sign│RES0│ PGD索引 │ PUD索引 │ PMD索引 │ PTE索引 │ 页内偏移 │          │
│ │    │    │ [47:39]│ [38:30]│ [29:21]│ [20:12]│ [11:0] │          │
│ └────┴────┴────────┴────────┴────────┴────────┴────────┴────────┘          │
│        │      9位     9位     9位     9位    12位   │
│        │       │       │       │       │       │    │
│        │       │       │       │       │       └─── 页内偏移 (0-4095)       │
│        │       │       │       │       └─────────── PTE索引 (512个条目)     │
│        │       │       │       └─────────────────── PMD索引 (512个条目)     │
│        │       │       └─────────────────────────── PUD索引 (512个条目)     │
│        │       └─────────────────────────────────── PGD索引 (512个条目)     │
│        └─────────────────────────────────────────── 每级索引位数=9          │
└─────────────────────────────────────────────────────────────────────────────┘

计算公式:
  每级表项数: PTRS_PER_PTE = 1 << (PAGE_SHIFT - 3) = 512  (4KB页)
  PGD索引: (VA >> 39) & 0x1FF
  PUD索引: (VA >> 30) & 0x1FF
  PMD索引: (VA >> 21) & 0x1FF
  PTE索引: (VA >> 12) & 0x1FF
```

来源: `arch/arm64/include/asm/pgtable-hwdef.h`
```c
#define ARM64_HW_PGTABLE_LEVELS(va_bits) (((va_bits) - 4) / (PAGE_SHIFT - 3))
// 对于 VA_BITS=48, PAGE_SHIFT=12:
// levels = (48 - 4) / (12 - 3) = 44 / 9 = 4 级

#define ARM64_HW_PGTABLE_LEVEL_SHIFT(n) ((PAGE_SHIFT - 3) * (4 - (n)) + 3)
// Level 0 (PGD): (9 * 4) + 3 = 39
// Level 1 (PUD): (9 * 3) + 3 = 30
// Level 2 (PMD): (9 * 2) + 3 = 21
// Level 3 (PTE): (9 * 1) + 3 = 12
```

### 3.2 页表遍历流程图

```
                    ┌──────────────────────────────────────────────────────────┐
                    │                    TTBR0_EL1 / TTBR1_EL1                 │
                    │                    (页表基址物理地址)                    │
                    └──────────────────────┬───────────────────────────────────┘
                                           │
                                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           Level 0: PGD (Page Global Directory)               │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ VA[47:39] ──→ 索引 ──→ PGD[索引] ──→ 提取下一级表地址 (PA)             │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  PGD条目格式:                                                               │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │ [47:12] = 下一级表物理地址 │ [1] = Table位(1) │ [0] = Valid(1)    │    │
│  └────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────┬───────────────────────────────────┘
                                           │
                                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           Level 1: PUD (Page Upper Directory)                │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ VA[38:30] ──→ 索引 ──→ PUD[索引] ──→ 提取下一级表地址 (PA)             │  │
│  │                     或 Block地址 (1GB Block映射时)                     │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  PUD条目格式 (Table):                                                       │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │ [47:12] = 下一级表物理地址 │ [1] = Table位(1) │ [0] = Valid(1)    │    │
│  └────────────────────────────────────────────────────────────────────┘    │
│  PUD条目格式 (Block - 1GB):                                                 │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │ [47:30] = 输出物理地址 │ AttrIdx/SH/AP/AF/nG │ [1]=0 │ [0]=Valid │    │
│  └────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────┬───────────────────────────────────┘
                                           │
                                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           Level 2: PMD (Page Middle Directory)               │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ VA[29:21] ──→ 索引 ──→ PMD[索引] ──→ 提取下一级表地址 (PA)             │  │
│  │                     或 Block地址 (2MB Block映射时)                     │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  PMD条目格式 (Table):                                                       │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │ [47:12] = 下一级表物理地址 │ [1] = Table位(1) │ [0] = Valid(1)    │    │
│  └────────────────────────────────────────────────────────────────────┘    │
│  PMD条目格式 (Block - 2MB):                                                 │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │ [47:21] = 输出物理地址 │ AttrIdx/SH/AP/AF/nG │ [1]=0 │ [0]=Valid │    │
│  └────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────┬───────────────────────────────────┘
                                           │
                                           ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│                           Level 3: PTE (Page Table Entry)                    │
│  ┌────────────────────────────────────────────────────────────────────────┐  │
│  │ VA[20:12] ──→ 索引 ──→ PTE[索引] ──→ 提取物理页地址 (PA)               │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  PTE条目格式 (Page - 4KB):                                                  │
│  ┌────────────────────────────────────────────────────────────────────┐    │
│  │ [47:12] = 物理页地址 │ AttrIdx/SH/AP/AF/nG/PXN/UXN │ [1]=1 │ [0]=1│    │
│  └────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────┬───────────────────────────────────┘
                                           │
                                           ▼
                    ┌──────────────────────────────────────────────────────────┐
                    │                        物理地址                          │
                    │            PA = PTE[47:12] : VA[11:0]                   │
                    └──────────────────────────────────────────────────────────┘
```

### 3.3 Linux内核页表遍历代码

来源: `arch/arm64/mm/mmu.c`

```c
int kern_addr_valid(unsigned long addr)
{
    pgd_t *pgdp;
    pud_t *pudp, pud;
    pmd_t *pmdp, pmd;
    pte_t *ptep, pte;

    // 检查地址是否在内核空间
    if ((((long)addr) >> VA_BITS) != -1UL)
        return 0;

    // Level 0: PGD
    pgdp = pgd_offset_k(addr);
    if (pgd_none(READ_ONCE(*pgdp)))
        return 0;

    // Level 1: PUD
    pudp = pud_offset(pgdp, addr);
    pud = READ_ONCE(*pudp);
    if (pud_none(pud))
        return 0;
    if (pud_sect(pud))  // 1GB Block
        return pfn_valid(pud_pfn(pud));

    // Level 2: PMD
    pmdp = pmd_offset(pudp, addr);
    pmd = READ_ONCE(*pmdp);
    if (pmd_none(pmd))
        return 0;
    if (pmd_sect(pmd))  // 2MB Block
        return pfn_valid(pmd_pfn(pmd));

    // Level 3: PTE
    ptep = pte_offset_kernel(pmdp, addr);
    pte = READ_ONCE(*ptep);
    if (pte_none(pte))
        return 0;

    return pfn_valid(pte_pfn(pte));
}
```

---

## 4. 从物理地址划分到开启页表的完整流程

### 4.1 整体流程概览

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         AArch64 启动流程                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────┐                                                        │
│  │  1. CPU上电     │◄── MMU=off, Cache=off                                 │
│  └────────┬────────┘                                                        │
│           ▼                                                                 │
│  ┌─────────────────┐                                                        │
│  │  2. stext()     │◄── head.S 入口                                        │
│  │   - preserve_boot_args                                                   │
│  │   - el2_setup   │    (保存启动参数, 初始化EL2/EL1)                      │
│  └────────┬────────┘                                                        │
│           ▼                                                                 │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │  3. __create_page_tables()                                       │       │
│  │     - 清空页表区域                                                │       │
│  │     - 创建恒等映射 (idmap_pg_dir)                                │       │
│  │     - 创建内核映射 (init_pg_dir)                                 │       │
│  └────────┬────────────────────────────────────────────────────────┘       │
│           ▼                                                                 │
│  ┌─────────────────┐                                                        │
│  │  4. __cpu_setup │◄── 初始化MAIR_EL1, TCR_EL1                           │
│  └────────┬────────┘                                                        │
│           ▼                                                                 │
│  ┌─────────────────┐                                                        │
│  │  5. __enable_mmu│◄── 设置TTBR0/1, 使能MMU                               │
│  └────────┬────────┘                                                        │
│           ▼                                                                 │
│  ┌─────────────────┐                                                        │
│  │  6. start_kernel│◄── 进入C代码世界                                      │
│  └─────────────────┘                                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 步骤详解

#### Step 1: 页表内存划分

来源: `arch/arm64/kernel/head.S`

```
内核启动时的内存布局:
┌─────────────────────────────────────────────────────────────────────────────┐
│ 物理地址空间                                                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────┐                                   │
│  │        idmap_pg_dir                 │ ◄── 恒等映射页表                  │
│  │        (恒等映射页表目录)            │     大小: PAGE_SIZE * N          │
│  └─────────────────────────────────────┘                                   │
│  ┌─────────────────────────────────────┐                                   │
│  │        init_pg_dir                  │ ◄── 初始内核页表                  │
│  │        (初始页表目录)                │     大小: PAGE_SIZE * N          │
│  └─────────────────────────────────────┘                                   │
│  ┌─────────────────────────────────────┐                                   │
│  │        内核镜像 (_text - _end)       │ ◄── 内核代码和数据                │
│  └─────────────────────────────────────┘                                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

Linux源码 (head.S):
```asm
__create_page_tables:
    mov x28, lr

    // 清空页表区域
    adrp x0, init_pg_dir
    adrp x1, init_pg_end
    sub x1, x1, x0
1:  stp xzr, xzr, [x0], #16
    stp xzr, xzr, [x0], #16
    stp xzr, xzr, [x0], #16
    stp xzr, xzr, [x0], #16
    subs x1, x1, #64
    b.ne 1b

    mov x7, SWAPPER_MM_MMUFLAGS   // 页表属性

    // 创建恒等映射
    adrp x0, idmap_pg_dir
    adrp x3, __idmap_text_start
    ...
    map_memory x0, x1, x3, x6, x7, x3, x4, x10, x11, x12, x13, x14

    // 创建内核映射
    adrp x0, init_pg_dir
    mov_q x5, KIMAGE_VADDR + TEXT_OFFSET
    ...
    map_memory x0, x1, x5, x6, x7, x3, x4, x10, x11, x12, x13, x14
```

#### Step 2: 创建页表条目

`map_memory` 宏展开:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    map_memory 宏执行流程                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  输入参数:                                                                   │
│    tbl  = 页表基地址                                                        │
│    vstart, vend = 虚拟地址范围                                              │
│    phys = 物理地址                                                          │
│    flags = 页表属性                                                         │
│                                                                             │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ 1. 计算PGD索引范围                                                     │ │
│  │    compute_indices vstart, vend, PGDIR_SHIFT                          │ │
│  │    populate_entries (填充PGD条目, 指向下一级)                          │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                          ▼                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ 2. 计算PUD索引范围 (如果4级页表)                                       │ │
│  │    compute_indices vstart, vend, PUD_SHIFT                            │ │
│  │    populate_entries (填充PUD条目, 指向下一级)                          │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                          ▼                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ 3. 计算PMD索引范围                                                     │ │
│  │    compute_indices vstart, vend, SWAPPER_TABLE_SHIFT                  │ │
│  │    populate_entries (填充PMD条目, 指向下一级)                          │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                          ▼                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │ 4. 计算PTE索引范围                                                     │ │
│  │    compute_indices vstart, vend, SWAPPER_BLOCK_SHIFT                  │ │
│  │    populate_entries (填充PTE条目, 包含物理地址和属性)                  │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Step 3: CPU初始化

来源: `arch/arm64/mm/proc.S`

```asm
ENTRY(__cpu_setup)
    tlbi vmalle1           // 无效化TLB
    dsb nsh

    // 使能FP/ASIMD
    mov x0, #3 << 20
    msr cpacr_el1, x0

    // 设置MAIR_EL1 (内存属性)
    ldr x5, =MAIR(0x00, MT_DEVICE_nGnRnE) | \
                MAIR(0x04, MT_DEVICE_nGnRE) | \
                MAIR(0x0c, MT_DEVICE_GRE) | \
                MAIR(0x44, MT_NORMAL_NC) | \
                MAIR(0xff, MT_NORMAL) | \
                MAIR(0xbb, MT_NORMAL_WT)
    msr mair_el1, x5

    // 设置TCR_EL1 (翻译控制寄存器)
    ldr x10, =TCR_TxSZ(VA_BITS) | TCR_CACHE_FLAGS | TCR_SMP_FLAGS | \
                TCR_TG_FLAGS | TCR_KASLR_FLAGS | TCR_ASID16 | \
                TCR_TBI0 | TCR_A1 | TCR_KASAN_FLAGS

    // 设置IPS (中间物理地址大小)
    tcr_compute_pa_size x10, #TCR_IPS_SHIFT, x5, x6

    msr tcr_el1, x10
    ret
ENDPROC(__cpu_setup)
```

#### Step 4: 使能MMU

来源: `arch/arm64/kernel/head.S`

```asm
ENTRY(__enable_mmu)
    // 检查页粒度是否支持
    mrs x2, ID_AA64MMFR0_EL1
    ubfx x2, x2, #ID_AA64MMFR0_TGRAN_SHIFT, 4
    cmp x2, #ID_AA64MMFR0_TGRAN_SUPPORTED
    b.ne __no_granule_support

    // 设置TTBR0 (恒等映射)
    adrp x2, idmap_pg_dir
    phys_to_ttbr x2, x2
    msr ttbr0_el1, x2

    // 设置TTBR1 (内核映射)
    phys_to_ttbr x1, x1
    offset_ttbr1 x1, x3
    msr ttbr1_el1, x1

    isb

    // 使能MMU (设置SCTLR_EL1.M位)
    msr sctlr_el1, x0
    isb

    // 无效化指令缓存
    ic iallu
    dsb nsh
    isb

    ret
ENDPROC(__enable_mmu)
```

### 4.3 完整初始化序列 (伪代码)

```c
// 1. 定义页表内存
pgd_t idmap_pg_dir[PTRS_PER_PGD];    // 恒等映射
pgd_t init_pg_dir[PTRS_PER_PGD];     // 初始内核页表

// 2. 创建页表
void __create_page_tables(void) {
    // 清空页表
    memset(init_pg_dir, 0, sizeof(init_pg_dir));

    // 创建恒等映射: VA = PA
    // 用于MMU使能后的平滑过渡
    for (addr = __idmap_text_start; addr < __idmap_text_end; addr += PAGE_SIZE) {
        create_mapping(idmap_pg_dir, addr, addr, PAGE_SIZE, PROT_KERNEL);
    }

    // 创建内核映射: VA = PA + KIMAGE_VOFFSET
    for (addr = _text; addr < _end; addr += PAGE_SIZE) {
        create_mapping(init_pg_dir, __va(addr), __pa(addr), PAGE_SIZE, PROT_KERNEL);
    }
}

// 3. 初始化系统寄存器
void __cpu_setup(void) {
    // 设置内存属性
    write_sysreg(MAIR_EL1, MAIR_VALUE);

    // 设置翻译控制
    write_sysreg(TCR_EL1, TCR_VALUE);

    // 无效化TLB
    asm volatile("tlbi vmalle1; dsb nsh");
}

// 4. 使能MMU
void __enable_mmu(void) {
    // 设置页表基址
    write_sysreg(TTBR0_EL1, __pa(idmap_pg_dir));
    write_sysreg(TTBR1_EL1, __pa(swapper_pg_dir));

    // 同步
    asm volatile("isb");

    // 使能MMU
    sctlr = read_sysreg(SCTLR_EL1);
    sctlr |= SCTLR_ELx_M | SCTLR_ELx_C | SCTLR_ELx_I;
    write_sysreg(SCTLR_EL1, sctlr);

    // 同步
    asm volatile("isb; ic iallu; dsb nsh; isb");
}
```

### 4.4 地址空间布局

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AArch64 虚拟地址空间布局                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  VA[63] = 0 (TTBR0)                    VA[63] = 1 (TTBR1)                  │
│  ┌──────────────────────────┐          ┌──────────────────────────┐        │
│  │    用户空间 (低地址)     │          │    内核空间 (高地址)     │        │
│  │                          │          │                          │        │
│  │  0x0000_0000_0000_0000   │          │  0xFFFF_0000_0000_0000   │        │
│  │         ↓                │          │         ↓                │        │
│  │    用户代码/数据         │          │    内核线性映射          │        │
│  │         ↓                │          │    (PAGE_OFFSET)         │        │
│  │                          │          │         ↓                │        │
│  │  0x0000_7FFF_FFFF_FFFF   │          │    vmalloc区域           │        │
│  │                          │          │         ↓                │        │
│  │                          │          │    vmemmap区域           │        │
│  │                          │          │         ↓                │        │
│  │                          │          │    PCI I/O空间           │        │
│  │                          │          │         ↓                │        │
│  │                          │          │    fixmap区域            │        │
│  │                          │          │         ↓                │        │
│  │                          │          │  0xFFFF_FFFF_FFFF_FFFF   │        │
│  └──────────────────────────┘          └──────────────────────────┘        │
│                                                                             │
│  地址划分由 TCR_EL1.T0SZ/T1SZ 控制                                         │
│  T0SZ = 64 - VA_BITS (用户空间大小)                                        │
│  T1SZ = 64 - VA_BITS (内核空间大小)                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

来源: `arch/arm64/include/asm/memory.h`
```c
#define VA_BITS         (CONFIG_ARM64_VA_BITS)
#define PAGE_OFFSET     (_PAGE_OFFSET(VA_BITS))
#define _PAGE_OFFSET(va)    (-(UL(1) << (va)))
```

---

## 参考资料

1. **Linux 5.4.29 源码**
   - `arch/arm64/include/asm/pgtable-hwdef.h` - 页表硬件定义
   - `arch/arm64/include/asm/pgtable.h` - 页表操作
   - `arch/arm64/include/asm/pgtable-prot.h` - 页表保护位
   - `arch/arm64/kernel/head.S` - 启动汇编代码
   - `arch/arm64/mm/proc.S` - CPU初始化
   - `arch/arm64/mm/mmu.c` - MMU管理

2. **ARM Architecture Reference Manual** (ARM DDI 0487)
   - Chapter D5: The AArch64 Virtual Memory System Architecture
   - Chapter D8: AArch64 Address Translation

---

*文档生成日期: 2024*
*基于 Linux kernel 5.4.29*