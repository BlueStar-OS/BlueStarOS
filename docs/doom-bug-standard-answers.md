# Doom Debug Standard Answers

这份文档只写“原理答案”，不写修复代码。

建议你以后遇到同类问题，先按这条事件流定位：

`ELF/auxv -> 用户初始栈 -> musl 启动 -> brk/mmap -> 页表/缺页 -> syscall ABI -> Doom 平台输入`

## 1. `AT_RANDOM` 不能是空指针

现象：
- `malloc` / `fbdoom` 很早就因为访问低地址或者奇怪地址崩掉。

根因：
- `musl` 很早就会用 `auxv[AT_RANDOM]` 指向的 16 字节随机块。
- 如果你把 `AT_RANDOM` 留成 `0`，或者没有把它回填到“用户栈里真实存在的一块 16 字节内存”，用户态启动期就会踩空。

Linux 参考：
- `/home/inkbottle/桌面/linux-5.4.29/fs/binfmt_elf.c:219-225`
- `/home/inkbottle/桌面/linux-5.4.29/fs/binfmt_elf.c:246-260`

你现在要自己补的入口：
- [task.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/task/task.rs:61)
- [task.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/task/task.rs:131)
- [task.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/task/task.rs:283)

## 2. `brk` 不能只看“PTE 现在是不是空”

现象：
- `sys_brk` 看起来成功了，但后面 heap / malloc metadata / guard page 地址互相踩。

根因：
- 你不能用“当前页表里有没有有效 PTE”来判断地址空间是不是空闲。
- 因为 `mmap` 的懒分配页、`PROT_NONE` guard page、保留区，可能已经占了虚拟地址，但此时还没有真正的有效 PTE。
- 所以 `brk` 扩堆必须按 `VMA/MapArea` 维度看冲突，不是按“当前有没有物理页”看冲突。

Linux 参考：
- `/home/inkbottle/桌面/linux-5.4.29/mm/mmap.c:259-265`
- `/home/inkbottle/桌面/linux-5.4.29/mm/mmap.c:2992-3006`

你现在要自己补的入口：
- [syscall.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/syscall/syscall.rs:787)

## 3. `MAP_FIXED` 的本质是“先拆旧映射，再建新映射”

现象：
- `mmap(MAP_FIXED)` 或 malloc guard page 看起来做了，但旧页还活着，随后出现诡异页错误。

根因：
- 你的 `unmap_range()` 如果只处理 `mmap area`，却跳过普通 `heap area`，那旧映射根本没真正拆掉。
- 后面再塞一个新的 fixed mapping，就会出现“同一虚拟页被两套语义管理”的坏状态。

Linux 参考：
- `/home/inkbottle/桌面/linux-5.4.29/mm/mmap.c:1726-1733`

你现在要自己补的入口：
- [memset.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/memory/memset.rs:1069)

## 4. `find_pte_vpn()` 返回了条目，不等于这个映射有效

现象：
- 有时你明明“翻译成功”了，但后面拿到的是垃圾物理地址，或者最后炸成内核页错误。

根因：
- 页表 walker 找到“最后一级 PTE 地址”，只代表“这个槽位存在”，不代表“这个 PTE 合法且 present/valid”。
- 所以 `translate()` / `translate_byvpn()` 还必须再检查 valid/present 语义。

Linux 参考：
- `/home/inkbottle/桌面/linux-5.4.29/arch/riscv/include/asm/pgtable.h:204-206`

你现在要自己补的入口：
- [address.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/arch/riscv64/memory/address.rs:277)

## 5. lazy `mmap` 不能只在 user trap 里补页

现象：
- `mmap()` 返回成功，但内核在 `sys_read` / `sys_write` / 复制用户缓冲区时，先一步访问了那块用户页，然后直接失败。

根因：
- 你只有“用户态触发 page fault 时”的补页逻辑。
- 但 syscall 里也会主动去摸用户缓冲区；如果这条路径不认识 lazy `mmap`，它就会把“还没建 PTE 的合法页”当成坏地址。

你现在要自己补的入口：
- [address.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/arch/riscv64/memory/address.rs:197)
- [address.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/arch/riscv64/memory/address.rs:237)

## 6. Doom 需要最小 Linux ABI，不是只有 `read/write`

现象：
- 你看到 `Unknown syscall id=65/113/134/135`。

根因：
- `musl + Doomgeneric` 至少会碰到：
- `readv`
- `clock_gettime`
- `rt_sigaction`
- `rt_sigprocmask`

Linux 参考：
- `/home/inkbottle/桌面/linux-5.4.29/fs/read_write.c:987-1043`
- `/home/inkbottle/桌面/linux-5.4.29/kernel/time/posix-stubs.c:72-103`
- `/home/inkbottle/桌面/linux-5.4.29/kernel/signal.c:3015-3043`
- `/home/inkbottle/桌面/linux-5.4.29/kernel/signal.c:4233-4255`

你现在要自己补的入口：
- [mod.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/syscall/mod.rs:165)

## 7. 固定 2 页用户栈，对 `musl + Doom` 太小

现象：
- 还没进业务逻辑，就先在用户栈附近 `StorePageFault`。

根因：
- 你的系统还没有 `grow-down stack`。
- 所以一旦初始栈太小，`musl` 启动过程自己就能把栈打穿。

Linux 参考：
- `/home/inkbottle/桌面/linux-5.4.29/mm/mmap.c:2549-2566`

你现在要自己补的入口：
- [memset.rs](/home/inkbottle/othersrc/BlueStarOS/kernel/src/memory/memset.rs:1380)

## 8. `Tab` 能用但 `WASD` 不能动，不是键位表问题

现象：
- `Tab` 有效，`WASD` 没反应，游戏却没卡死。

根因：
- 你现在的 `/dev/keyboard` 本质上是 UART 字节流，不是真实键盘的按下/抬起事件流。
- 如果平台层把 `WASD` 翻成“立刻 keydown，再立刻 keyup”，那一次性键会正常，持续动作键会失效。

你现在要自己补的入口：
- [doomgeneric_soso.c](/home/inkbottle/othersrc/doomgeneric/doomgeneric/doomgeneric_soso.c:209)

## 最后一句

以后别先问“是不是这个函数错了”，先问：

1. 这是 ABI 问题，还是地址空间问题，还是设备语义问题？
2. 这个地址“没物理页”是非法，还是只是 lazy？
3. 现在判断空闲/有效，用的是 `PTE` 语义，还是 `VMA/MapArea` 语义？
4. 这个输入/输出流，传的是“字节”，还是“事件”？

你把这四句问熟了，调试能力会涨得非常快。
