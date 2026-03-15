# 硬件中断机制详解

## 你的理解哪里对、哪里需要修正

你说的大方向是对的：设备产生信号 → CPU 感知 → OS 处理 → 清除中断。
但中间少了一个关键角色：**中断控制器**。现代系统中，设备的中断线**不是直接连到 CPU** 的。

---

## 全景图：三个角色

```
┌─────────────┐    IRQ 线     ┌──────────────────┐   一根线    ┌──────────┐
│  外设设备     │─────────────→│   中断控制器       │───────────→│   CPU    │
│ (UART/键盘等) │              │ (PLIC / GIC 等)   │            │          │
└─────────────┘              └──────────────────┘            └──────────┘
     ①产生中断                    ②仲裁+转发                  ③响应中断
```

**为什么需要中断控制器？**
- 一个系统有几十上百个外设，但 CPU 的中断引脚就那么几个
- 中断控制器负责：**汇聚、排优先级、转发**给 CPU

---

## 详细流程（以 RISC-V + PLIC + UART 为例）

```
 时间线 ──────────────────────────────────────────────────────→

 ┌──────┐         ┌──────┐         ┌──────┐
 │ UART │         │ PLIC │         │ CPU  │
 └──┬───┘         └──┬───┘         └──┬───┘
    │                 │                │
    │  ① 用户按下键盘   │                │
    │  UART 收到数据    │                │
    │                 │                │
    │ ── IRQ #10 ───→ │                │   ② UART 拉低 IRQ 线（电平触发）
    │  (拉低电平)       │                │      告诉 PLIC："我有事！"
    │                 │                │
    │                 │ ── 外部中断 ──→ │   ③ PLIC 检查优先级，
    │                 │  (拉高 SIP.SEIP) │      向 CPU 发出外部中断信号
    │                 │                │
    │                 │                │   ④ CPU 检测到中断：
    │                 │                │      - 保存 PC 到 sepc
    │                 │                │      - 跳转到 stvec（中断入口）
    │                 │                │
    │                 │                │   ⑤ OS 中断处理程序运行：
    │                 │                │      - 读 PLIC claim 寄存器
    │                 │  ←── claim ─── │      → 得到 IRQ #10
    │                 │                │      → 知道是 UART
    │                 │                │
    │                 │                │   ⑥ 调用 UART 驱动：
    │  ←── 读数据 ──── │ ←──────────── │      - 读 UART 数据寄存器
    │  ──→ 返回 'A' ─→ │ ───────────→  │      - 取走按键数据 'A'
    │                 │                │
    │  (IRQ 线自动拉高) │                │   ⑦ 数据被读走后，
    │                 │                │      UART 自动释放 IRQ 线
    │                 │                │
    │                 │  ←── complete ─ │   ⑧ OS 写 PLIC complete 寄存器
    │                 │                │      告诉 PLIC："#10 处理完了"
    │                 │                │
    │                 │                │   ⑨ CPU 执行 sret，
    │                 │                │      恢复 sepc，回到被打断的代码
    ▼                 ▼                ▼
```

---

## 硬件连线的真实样子

```
                        中断控制器 (PLIC)
                    ┌─────────────────────┐
  UART ── IRQ #10 ─→│ 源 10               │
  GPIO ── IRQ #7  ─→│ 源 7    ┌────────┐  │      ┌──────────┐
  定时器 ─ IRQ #3  ─→│ 源 3    │优先级   │──│─────→│ CPU 核心0 │
  网卡 ── IRQ #15 ─→│ 源 15   │仲裁逻辑 │  │      └──────────┘
  SPI  ── IRQ #20 ─→│ 源 20   └────────┘  │      ┌──────────┐
  ...               │                     │─────→│ CPU 核心1 │
                    └─────────────────────┘      └──────────┘
```

关键点：
- 每个外设有一个固定的 **IRQ 编号**（硬件设计时决定的，写在设备树/手册里）
- PLIC 内部为每个 IRQ 源维护 **优先级** 和 **使能位**
- PLIC 只用 **一根线** 连到每个 CPU 核心（外部中断引脚）

---

## 中断触发方式对比

```
电平触发（Level Triggered）—— UART 等大多数设备用这种
─────────┐                 ┌─────────
  高电平   │    低电平=有中断  │  高电平
（空闲）   └─────────────────┘ （中断清除）
           ↑                 ↑
        设备拉低           驱动读走数据后
        表示有中断          设备自动拉高


边沿触发（Edge Triggered）—— 某些 GPIO/按键用这种
──────────┐  ┌───────────────────────
           │  │
           └──┘
           ↑
        下降沿 = 产生一次中断
        不管后续电平如何
```

---

## CPU 内部的中断相关寄存器（RISC-V S-mode）

OS 内核运行在 S-mode，所以用的是 `s` 开头的这套寄存器。
（`m` 开头的那套是 M-mode 的，由 SBI/OpenSBI 固件使用，OS 一般不直接操作。）

```
┌─ CPU 内部（S-mode 寄存器）─────────────────────────────────┐
│                                                           │
│  sstatus.SIE ─── 全局中断开关（0=关中断，1=开中断）          │
│                                                           │
│  sie ─────────── 中断使能寄存器                             │
│    ├─ SSIE (bit 1)  软件中断使能                            │
│    ├─ STIE (bit 5)  定时器中断使能                          │
│    └─ SEIE (bit 9)  外部中断使能 ← PLIC 的信号走这里         │
│                                                           │
│  sip ─────────── 中断挂起寄存器（只读，反映当前状态）          │
│    ├─ SSIP (bit 1)  有软件中断挂起？                        │
│    ├─ STIP (bit 5)  有定时器中断挂起？                      │
│    └─ SEIP (bit 9)  有外部中断挂起？ ← PLIC 拉高这一位       │
│                                                           │
│  stvec ────────── 中断入口地址（OS 设置的跳转目标）           │
│  sepc ─────────── 被打断时的 PC（中断返回地址）               │
│  scause ───────── 中断原因编号                              │
│                                                           │
└───────────────────────────────────────────────────────────┘
```

---


## 一句话总结

```
外设 ──IRQ线──→ 中断控制器 ──一根线──→ CPU ──跳转──→ OS中断处理
                (汇聚+优先级)         (保存现场)     (dispatch到驱动)
```

你原来的理解 **"UART 直接连 CPU"** 少了中间的中断控制器。
实际上是：**UART → PLIC → CPU → OS → 驱动**。

驱动处理完后也不是驱动"拉高电平"，而是：
1. 驱动**读走数据** → 设备自动释放 IRQ 线
2. 驱动**写 PLIC complete 寄存器** → 告诉 PLIC 这个中断处理完了













 先把 16550 UART 的中断机制讲清楚，然后定位真正的问题。

  16550 UART 中断机制

  UART 内部寄存器（偏移量相对于 base 0x1000_0000）：

  偏移  寄存器   作用
  +0    RBR/THR  接收缓冲(读) / 发送保持(写)
  +1    IER      中断使能寄存器
  +2    IIR/FCR  中断识别(读) / FIFO控制(写)
  +5    LSR      线路状态寄存器
  +6    MSR      Modem状态寄存器

  IER（中断使能）控制哪些事件能产生中断：

  IER 寄存器：
    bit 0 = RX 数据可用中断
    bit 1 = TX 保持寄存器空中断
    bit 2 = 接收线路状态中断（溢出/校验/帧错误）
    bit 3 = Modem 状态变化中断

  IIR（中断识别）告诉你当前是什么中断：

  IIR 寄存器（只读）：
    bit 0 = 0: 有中断挂起    1: 无中断挂起
    bit[3:1] = 中断类型：
      0b110 = 接收线路状态错误（最高优先级）  → 读 LSR 清除
      0b010 = RX 数据可用                    → 读 RBR 清除
      0b110 = 字符超时（FIFO模式）            → 读 RBR 清除
      0b001 = TX 保持寄存器空                → 读 IIR 或写 THR 清除
      0b000 = Modem 状态变化（最低优先级）    → 读 MSR 清除

  关键点：16550 的 IRQ 是电平触发的。只要有任何一个使能的中断源处于活跃状态，IRQ
  线就一直拉低。必须把所有活跃的中断源都清掉，IRQ 才会释放。

  你的真正问题

  问题不在 handler 怎么清中断，而是轮询和中断在打架：

  时间线：

  用户按键 → UART 收到数据 → RBR 有数据 → IRQ 拉低 → PLIC 记录

      ┌─────── 两条路径竞争同一个 RBR ───────┐
      │                                      │
      ▼                                      ▼
   定时器中断先到                         外部中断后到
   → 调度 shell                          → keyboard_interrupt_handler
   → tty get_char()                      → uart::getc()
   → uart::getc() 读走了 RBR ✓           → RBR 已空，返回 None ✗
   → 字符被轮询消费                       → 但 PLIC 已经 claim 了 IRQ 10
                                         → complete(10)
                                         → UART 此时 IRQ 线已释放
                                         → 看起来没问题...

  但如果 UART 的 THRE（TX空）中断被意外触发：
   → putc() 写了 THR → TX 发完 → THR 空 → 如果 IER bit1=1 → IRQ 又拉低
   → 无限循环

  让我确认一下你的 IER 到底是什么值——在 PLIC init 之后读回来看：

● Read 1 file (ctrl+o to expand)

● 代码是对的。加个诊断：在 enable_uart_rx_interrupt 之后读回 IER 和 IIR 的值，看看 UART 的真实状态：

● Update(kernel\src\driver\keyboard\mod.rs)
  ⎿  Added 11 lines, removed 1 line
      29              ier.write_volatile(0x01);
      30          }
      31      }
      32 -    crate::kprintln!("[Keyboard] UART RX interrupt turn");
      32 +    crate::kprintln!("[Keyboard] UART RX interrupt enabled");
      33 +    // 诊断：读回 UART 寄存器确认状态
      34 +    unsafe {
      35 +        let ier_val = ((UART_BASE + 1) as *const u8).read_volatile();
      36 +        let iir_val = (UART_IIR as *const u8).read_volatile();
      37 +        let lsr_val = (UART_LSR as *const u8).read_volatile();
      38 +        crate::kprintln!(
      39 +            "[UART] IER={:#04x} IIR={:#04x} LSR={:#04x}",
      40 +            ier_val, iir_val, lsr_val
      41 +        );
      42 +    }
      43  }
      44
      45  /// UART 中断处理入口

● 跑一下，把 [UART] IER=... IIR=... LSR=... 那行贴给我看。这能告诉我们：
  - IER 是不是真的只有 bit 0（应该是 0x01）
  - IIR 是否已经有中断挂起（bit 0 应该是 1 = 无挂起）
  - LSR 有没有错误标志




 破案了。这不是 UART/PLIC 的问题，是内核态中断屏蔽的问题。

  根因

  你的 shell 进程在 sys_read → get_char() 循环里一直待在内核态：

  用户态 (SIE=1, 中断可触发)
    │
    │ ecall (sys_read)
    ▼
  内核态 (SIE=0, 中断被屏蔽) ← 卡在这里出不去
    │
    └→ get_char() {
         loop {
             check buffer → 空
             suspend_and_run_task() → __switch → 切回来 → 还是内核态!
             // SIE 始终 = 0，外部中断永远进不来
             // buffer 永远是空的
             // 死锁！
         }
       }

  suspend_and_run_task() 做的是内核上下文切换（__switch 保存/恢复 ra, sp, s0-s11），不经过
  sret，所以进程永远不会回到用户态，sstatus.SIE 永远是 0，外部中断永远被屏蔽。

  加了轮询 uart::getc() 后能用，是因为轮询直接读 UART 硬件寄存器，根本不需要中断。

  解决方案

  在 get_char 等待循环里临时开启内核态中断，让外部中断能打进来：

● Read 1 file (ctrl+o to expand)

● Update(kernel\src\fs\component\tty\tty.rs)
  ⎿  Added 1 line
       4  use crate::task::TASK_MANAER;
       5  use crate::fs::vfs::{File, OpenFlags, VfsFsError};
       6  use crate::driver::uart;
       7 +use riscv::register::sstatus;
       8
       9
      10  pub const FD_TYPE_STDIN: usize = 0;

● Update(kernel\src\fs\component\tty\tty.rs)

* Envisioning… (1m 49s · ↑ 3.3k tokens)