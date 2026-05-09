# PCI 桥转发实现笔记

这份笔记只讲一件事：`PCI-to-PCI bridge` 为什么会挡住下游设备的 BAR 访问，以及你应该怎样自己把它写出来。

目标不是“看懂”，而是你看完以后能自己把 `Type 1 bridge forwarding` 写进 `kernel/src/driver/pcie/mod.rs`。

---

## 1. 先抓住本质

桥转发问题的本质不是“设备没枚举到”，而是：

1. `ECAM` 配置空间访问和 `BAR MMIO` 访问不是一条路径。
2. 你能通过 `ECAM` 扫到下游设备，不代表 CPU 发往 BAR 的内存事务能穿过桥。
3. `Type 1 bridge` 默认会用自己的窗口寄存器过滤下游地址范围。
4. 如果桥窗口没配，或者 `PCI_COMMAND_MEMORY` 没开，下游 BAR 就像“不存在”。

最典型现象：

- 配置空间里能扫到 `1234:1111`
- BAR 地址也已经分配出来
- 但访问 `BAR2` 读回 `0xffff`
- `bochs id` 读成 `0xffff`
- `xres/yres/bpp` 都变成 `65535`

这时优先怀疑桥转发，不要先怀疑显卡 modeset。

---

## 2. 你这次代码里，桥转发应该插在哪

位置在：

- [kernel/src/driver/pcie/mod.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/driver/pcie/mod.rs:760)

你当前桥扫描主流程是：

```text
发现 bridge
-> 分配 secondary bus
-> 先写 provisional bus number
-> 递归扫描 child bus
-> 得到真实 subordinate
-> 回来重写 bus number
-> 这里就是你要补 bridge forwarding 的地方
```

也就是：

```text
write_bridge_bus_numbers(... secondary, secondary)
scan_bus_recursive(secondary)
write_bridge_bus_numbers(... secondary, subordinate)
setup_bridge_forwarding_here(...)
```

注意顺序不能反：

- 你必须先知道这个桥下面到底挂了哪些设备、它们的 BAR 落在哪里
- 才能反推出这个桥的窗口应该覆盖哪一段地址

---

## 3. 先记住三个概念

### 3.1 primary / secondary / subordinate

对一个 Type 1 bridge：

- `primary bus`：桥所在的上游总线
- `secondary bus`：桥直接连接的下游第一条总线
- `subordinate bus`：这个桥下面整个子树能到达的最大总线号

如果桥下面只有一个设备，没有再挂桥：

```text
primary = 0
secondary = 2
subordinate = 2
```

如果桥下面还有桥，最深到 bus 3：

```text
primary = 0
secondary = 2
subordinate = 3
```

`subordinate` 的意义不是“显示信息”，而是告诉硬件：

```text
这个桥负责转发到哪些 bus number
```

但光有 bus number 还不够，BAR 地址事务还要额外看窗口寄存器。

### 3.2 桥窗口不是 BAR

下游端点设备自己的 BAR 是“设备解码范围”。

桥上的 `PCI_MEMORY_BASE/LIMIT` 是“桥允许哪些地址穿过去”。

所以访问路径是两段过滤：

```text
CPU 发出地址 A
-> 父桥判断 A 是否落在自己的 forwarding window
-> 如果是，继续往 child bus 发
-> 端点设备判断 A 是否落在自己的 BAR
-> 如果是，设备响应
```

少任何一段都不行。

### 3.3 `PCI_COMMAND_MEMORY` 不是 `PCI_COMMAND_MASTER`

- `PCI_COMMAND_MEMORY`：允许设备解码 memory space，桥也靠它决定是否转发 memory transaction
- `PCI_COMMAND_IO`：允许 I/O space
- `PCI_COMMAND_MASTER`：允许设备自己发 DMA，当 bus master

你的 CPU 去读显卡 BAR，不需要 `MASTER`，需要的是：

- 端点设备的 `PCI_COMMAND_MEMORY`
- 中间桥的 `PCI_COMMAND_MEMORY`

QEMU 的桥实现就是这么做的，见：

- `/home/inkbottle/othersrc/qemusrc/qemu/hw/pci/pci_bridge.c:193-206`

里面 `cmd & PCI_COMMAND_MEMORY` 直接决定 memory alias 是否启用。

---

## 4. 最小可用版本只需要搞懂这两个寄存器

Linux 头文件定义：

- `/home/inkbottle/桌面/linux-5.4.29/include/uapi/linux/pci_regs.h:140-143`

```c
#define PCI_MEMORY_BASE         0x20
#define PCI_MEMORY_LIMIT        0x22
#define PCI_MEMORY_RANGE_MASK   (~0x0fUL)
```

这两个寄存器属于 Type 1 header。

它们共同组成一个 32 位布局：

```text
offset 0x20 low 16 bits  = Memory Base
offset 0x22 high 16 bits = Memory Limit
```

Linux 写法在：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/setup-bus.c:609-626`

关键编码公式：

```text
encoded_base  = (window_start >> 16) & 0xfff0
encoded_limit =  window_end         & 0xfff00000
dword = encoded_base | encoded_limit
```

关键解码公式：

Linux 在：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/probe.c:437-457`

```text
decoded_start = (mem_base_lo  & 0xfff0) << 16
decoded_limit = (mem_limit_lo & 0xfff0) << 16 | 0x000f_ffff
```

你要立刻记住一件事：

```text
普通 memory bridge window 的粒度是 1 MiB
```

所以：

- 起点必须向下按 1 MiB 对齐
- 终点必须向上按 1 MiB 对齐，再减 1

---

## 5. 你这次 QEMU VGA 的具体例子

你那次扫出来的是：

```text
VGA BAR0 framebuffer base=0x4100_0000 size=0x0100_0000
VGA BAR2 mmio        base=0x4200_0000 size=0x0000_1000
```

先算每个 BAR 的结束地址：

```text
BAR0 end = 0x4100_0000 + 0x0100_0000 - 1 = 0x41ff_ffff
BAR2 end = 0x4200_0000 + 0x0000_1000 - 1 = 0x4200_0fff
```

整个子树覆盖范围先取并集：

```text
raw_start = min(0x4100_0000, 0x4200_0000) = 0x4100_0000
raw_end   = max(0x41ff_ffff, 0x4200_0fff) = 0x4200_0fff
```

再按 1 MiB 桥窗口粒度对齐：

```text
window_start = align_down(0x4100_0000, 0x0010_0000) = 0x4100_0000
window_end   = align_up(0x4200_1000, 0x0010_0000) - 1 = 0x420f_ffff
```

然后编码：

```text
encoded_base  = (0x4100_0000 >> 16) & 0xfff0 = 0x4100
encoded_limit =  0x420f_ffff        & 0xfff00000 = 0x4200_0000
dword = 0x4200_0000 | 0x0000_4100 = 0x4200_4100
```

这就是你应该写到桥 `0x20/0x22` 那个 dword 里的值。

---

## 6. 为什么“能扫到设备”但“BAR 读出来全是 0xffff”

因为这两种访问不走一条路：

### 6.1 配置空间访问

```text
CPU
-> ECAM 地址
-> Host bridge 直达配置空间
-> 你能读 vendor/device/header/class
```

### 6.2 BAR MMIO 访问

```text
CPU
-> 普通 MMIO 地址，比如 0x4200_0000
-> Root complex / parent bus
-> 中间 bridge 先看:
   1. PCI_COMMAND_MEMORY 是否打开
   2. 地址是否在 PCI_MEMORY_BASE/LIMIT 里
-> 通过后才会下发给 child bus
-> 端点设备再看 BAR 是否命中
```

所以：

```text
ECAM 成功 != BAR 一定可访问
```

---

## 7. 你应该自己写出的最小实现步骤

先只做普通 memory window，不碰 prefetchable / I/O。

### Step 1. 枚举完 child bus 后，收集子树里所有 memory BAR

输入：

- `secondary_bus`
- `subordinate_bus`
- 全局 `PCIE_DEVICES`

遍历条件：

```text
secondary_bus <= device.bus_number <= subordinate_bus
```

只收：

- `Memory32`
- `Memory64`

先别收：

- `Io`

过滤掉：

- `size == 0`

最后拿到：

```text
window_start = 所有 memory BAR base 的最小值
window_end   = 所有 memory BAR (base + size - 1) 的最大值
```

### Step 2. 对桥窗口做 1 MiB 对齐

```text
aligned_start = align_down(window_start, 1 MiB)
aligned_end   = align_up(window_end + 1, 1 MiB) - 1
```

### Step 3. 编码进 `PCI_MEMORY_BASE/LIMIT`

```text
bridge_mmio_dword =
    ((aligned_start >> 16) & 0xfff0)
  | ( aligned_end         & 0xfff00000)
```

然后：

```text
cfg_write32(bus, dev, func, PCI_MEMORY_BASE, bridge_mmio_dword)
```

### Step 4. 打开 `PCI_COMMAND_MEMORY`

先读旧命令字：

```text
cmd = cfg_read16(... PCI_COMMAND)
```

再写回：

```text
cmd |= PCI_COMMAND_MEMORY
cfg_write16(... PCI_COMMAND, cmd)
```

### Step 5. 读回校验

至少做两个验证：

1. 读回 `PCI_MEMORY_BASE/LIMIT`
2. 解码后打印成 `[start-end]`

如果你写进去的是 `0x42004100`，读回就不该还是 `0x0000fff0`。

---

## 8. 先不要一上来就追求“完整实现”

第一次自己写，只做下面这个最小目标：

```text
普通 memory window
+ 打开 PCI_COMMAND_MEMORY
```

这已经足够把 QEMU VGA 这种 case 打通。

完整版下一步再加：

1. `prefetchable memory window`
2. `64-bit prefetchable upper32`
3. `I/O window`
4. “空窗口”时写 disabled 编码
5. 按资源属性拆分普通 memory 和 prefetchable memory

Linux 对应参考：

- 普通 memory window：
  `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/setup-bus.c:609-626`
- prefetchable window：
  `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/setup-bus.c:628-660`
- 普通 memory window 解码：
  `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/probe.c:437-457`
- prefetchable window 解码：
  `/home/inkbottle/桌面/linux-5.4.29/drivers/pci/probe.c:459-490`

---

## 9. 你自己写时最容易犯的错

### 错 1. 把桥窗口和设备 BAR 当成一回事

不对。

- BAR 是端点设备自己的 decode
- bridge window 是上游过滤器

### 错 2. 只分配 BAR，不配置桥

这就是你这次最核心的问题。

症状：

```text
BAR 地址有了
但是 MMIO 读出来全是 0xffff
```

### 错 3. 开了 `MASTER`，以为就能访问 BAR

不对。

CPU 访问 BAR 看的是 `MEMORY` decode，不是 `MASTER`。

### 错 4. 没按 1 MiB 对齐桥窗口

桥 window 编码不是字节粒度。

如果你直接把 `0x4200_0fff` 生塞进去，编码就错了。

### 错 5. 在递归扫描 child bus 之前就配置桥 window

不对。

你那时根本还不知道 child subtree 最终占用了哪些 BAR。

顺序必须是：

```text
scan child
-> 汇总 child resources
-> 配桥窗口
```

### 错 6. 没做“写后读回”

桥寄存器是最该做 readback 的地方。

因为它的编码不是直观地址，而是压缩过的 bitfield。

---

## 10. 你可以直接照着写的实现骨架

下面不是现成答案，是你应该自己填完的骨架。

```rust
// 1. 扫描 child bus 完成后调用
fn setup_type1_bridge_forwarding(
    bridge_bus: u8,
    bridge_dev: u8,
    bridge_func: u8,
    secondary_bus: BusNo,
    subordinate_bus: BusNo,
) {
    // Step A: 收集 secondary..=subordinate 子树里的 BAR 资源
    // 只先收 Memory32 / Memory64

    // Step B: 求 raw_start / raw_end

    // Step C: 按 1 MiB 对齐
    // aligned_start = align_down(...)
    // aligned_end   = align_up(...)-1

    // Step D: 编码进 PCI_MEMORY_BASE/LIMIT
    // let bridge_mmio_dword = ...
    // cfg_write32(..., PCI_MEMORY_BASE, bridge_mmio_dword);

    // Step E: 打开 PCI_COMMAND_MEMORY
    // let command = cfg_read16(..., PCI_COMMAND);
    // cfg_write16(..., PCI_COMMAND, command | PCI_COMMAND_MEMORY);

    // Step F: 读回校验并打印日志
}
```

如果你第一次要自己写，我建议你拆成两个函数：

```text
collect_bridge_memory_window(...)
setup_bridge_memory_window(...)
```

因为这两个阶段的脑子是不一样的：

- 前者是“资源汇总”
- 后者是“寄存器编码”

拆开更不容易乱。

---

## 11. 你写完以后怎么验

### 11.1 先验桥寄存器

你应该打印：

- 原始写入 dword
- 读回 dword
- 解码后的 `window_start/window_end`

### 11.2 再验设备 BAR

对 QEMU VGA：

- `BAR0` framebuffer
- `BAR2` bochs mmio

### 11.3 最后验功能性

如果桥转发通了，典型变化是：

- `bochs id` 不再是 `0xffff`
- 能读到合法 `0xb0c0..0xb0c5`
- modeset 后 `enable/xres/yres/bpp` 不再全是 `65535`

---

## 12. 你这次应该带走的脑图

```text
枚举到桥
-> 写 primary/secondary/subordinate
-> 扫描 child subtree
-> 汇总 child subtree 的 BAR 资源
-> 计算桥窗口
-> 写 PCI_MEMORY_BASE/LIMIT
-> 开 PCI_COMMAND_MEMORY
-> 读回校验
-> 再访问下游 BAR
```

只要你以后碰到：

```text
“桥后设备能枚举，但 BAR 访问失败”
```

就按这个脑图查。

---

## 13. 这次保留下来的“低学习价值工具”

我没有删掉这些基础辅助，因为它们不是桥转发的核心难点：

- `write_bridge_bus_numbers(...)`
- `align_down(...)`
- `align_up(...)`

真正有学习价值、你应该自己重写的是：

- 如何从 child subtree 汇总资源
- 如何把资源编码成 bridge window
- 何时打开 `PCI_COMMAND_MEMORY/IO`
- 如何区分普通 memory / prefetchable / I/O

---

## 14. 最后给你的自检题

你看完后，应该能不用翻我这份文档，自己回答这 6 个问题：

1. 为什么能扫到下游设备，但 BAR 还是读不到？
2. `PCI_COMMAND_MEMORY` 和 `PCI_COMMAND_MASTER` 的区别是什么？
3. 为什么桥窗口一定要在递归扫描 child bus 之后再写？
4. `PCI_MEMORY_BASE/LIMIT` 为什么是 1 MiB 粒度？
5. `0x4100_0000..0x4200_0fff` 为什么最终要编码成覆盖到 `0x420f_ffff`？
6. 如果 `bochs id` 又读成 `0xffff`，你第一时间要查哪三个点？

如果这 6 个问题你都能直接答出来，你就已经可以自己把第一版桥转发写出来了。
