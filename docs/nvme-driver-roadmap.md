# BlueStarOS NVMe PCIe 驱动落地教程

这份文档只讲一件事：  
在 **你当前这棵 BlueStarOS master** 上，怎样把一个 NVMe PCIe 控制器接成 `BlockDevTrait`，最后让 `RootFs::init_rootfs()` 把它当系统块设备使用。

这不是“看懂 NVMe”的文档，而是“按这个顺序写，就能把第一版驱动做出来”的施工说明。

## 0. 先认清当前内核的接入点

你现在这棵树里，关键链路已经有了：

1. PCIe 枚举：
   `kernel/src/driver/pcie/mod.rs:1036-1046`
   这里会把每个 BDF 的扫描结果注册进 `PCIE_DEVICES`。

2. PCIe host bridge probe 入口：
   `kernel/src/driver/pcie/mod.rs:1060-1067`
   `pci_probe_callback()` 会在 DTB probe 时执行总线扫描。

3. 块设备统一注册点：
   `kernel/src/fs/vfs/mod.rs:58`
   `register_global_block_device(device)`

4. VFS 根文件系统消费块设备的入口：
   `kernel/src/fs/vfs/root/block_device.rs:311`
   `RootFs::scan_and_build_vblock_device()`

5. VFS 到块设备的抽象已经解耦：
   `kernel/src/fs/vfs/vblock.rs`
   这里只依赖 `BlockDevTrait`，不依赖 `VirtBlk`。

结论很关键：

- 你**不需要改 VFS 模型**。
- 你真正要做的是：  
  `PCIe 枚举结果 -> NvmeController -> NvmeBlockDevice -> register_global_block_device()`

## 1. 第一版不要贪，功能边界必须砍干净

第一版目标只做：

1. 匹配 NVMe PCIe 控制器。
2. BAR0 MMIO。
3. 打开 `PCI_COMMAND_MEMORY | PCI_COMMAND_MASTER`。
4. 控制器 reset / enable。
5. 1 对 Admin Queue。
6. `Identify Controller`。
7. `Identify Namespace`。
8. 1 对 I/O Queue。
9. polling 完成，不做中断。
10. 只做 PRP，不做 SGL。
11. 只支持 namespace 1。
12. 先支持连续物理缓冲区。

第一版**明确不做**：

1. MSI / MSI-X
2. 多 I/O queue
3. SGL
4. metadata / integrity
5. 多 namespace 扫描
6. controller memory buffer
7. shadow doorbell buffer
8. blk-mq 风格调度

你现在最需要的是一条“短而闭合”的路径，不是 Linux 完整功能。

## 2. 本机 QEMU 已确认支持 NVMe

我在你机器上直接查了本地 QEMU：

- `qemu-system-riscv64 -version`
  返回 `QEMU emulator version 10.0.9`
- `qemu-system-riscv64 -device help`
  包含：
  - `pcie-root-port`
  - `nvme`
  - `nvme-ns`
  - `nvme-subsys`

所以你可以直接在 QEMU 上先把 NVMe 驱动做出来。

最小示例参数：

```makefile
-drive file=nvme.img,if=none,format=raw,id=nvme0 \
-device pcie-root-port,id=rp2,chassis=2 \
-device nvme,serial=bsosnvme,drive=nvme0,bus=rp2
```

如果你暂时只想调 NVMe，建议先把 virtio-blk 的盘参数去掉，避免两个块设备同时注册时混淆日志。

## 3. 你该在 BlueStarOS 里新增哪些文件/调用

最小修改集是：

1. `kernel/src/driver/nvme/mod.rs`
   写 NVMe 控制器、队列、命令格式、轮询提交逻辑。

2. `kernel/src/driver/mod.rs`
   导出 `pub mod nvme;`

3. `kernel/src/driver/pcie/mod.rs`
   在 `pci_probe_callback()` 里，`scan_bus_recursive(BusNo::ROOT)` 之后调用：
   `crate::driver::nvme::probe_registered_pcie_nvme_devices()`

4. 不用改 `kernel/src/fs/vfs/vblock.rs`
   因为它已经只依赖 `BlockDevTrait`。

5. 不用改 `kernel/src/fs/vfs/root/block_device.rs`
   因为它已经从 `GLOBAL_BLOCKS` 自动创建设备文件。

## 4. 先学 Linux 的哪几段，按什么顺序抄思路

### 4.1 控制器结构与队列结构

先看：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/pci.c:90-131`
- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/pci.c:163-192`

你不用一比一照抄 Linux 的 `struct nvme_dev`，但你必须看懂第一版真正需要哪些字段：

1. BAR0 基址
2. doorbell stride
3. admin SQ/CQ 物理地址
4. io SQ/CQ 物理地址
5. sq_tail
6. cq_head
7. cq_phase
8. q_depth
9. namespace block size
10. namespace total blocks

### 4.2 MMIO 寄存器与位定义

看：

- `/home/inkbottle/桌面/linux-5.4.29/include/linux/nvme.h:89-166`

第一版必须实现的寄存器/位：

1. `NVME_REG_CAP = 0x0000`
2. `NVME_REG_VS = 0x0008`
3. `NVME_REG_CC = 0x0014`
4. `NVME_REG_CSTS = 0x001c`
5. `NVME_REG_AQA = 0x0024`
6. `NVME_REG_ASQ = 0x0028`
7. `NVME_REG_ACQ = 0x0030`
8. `NVME_CC_ENABLE`
9. `NVME_CSTS_RDY`
10. `NVME_CAP_MQES(cap)`
11. `NVME_CAP_MPSMIN(cap)`
12. `NVME_CC_MPS_SHIFT`
13. `NVME_CC_IOSQES`
14. `NVME_CC_IOCQES`

### 4.3 控制器 reset / enable 流程

看：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/core.c:2060-2147`

你必须按这个状态机来：

```text
读 CAP
  ↓
如果 CC.EN=1，先 disable
  ↓
等待 CSTS.RDY = 0
  ↓
分配 admin SQ/CQ 内存
  ↓
写 AQA / ASQ / ACQ
  ↓
构造 CC:
  CSS = NVM
  MPS = 4KiB
  IOSQES = 64B
  IOCQES = 16B
  EN = 1
  ↓
等待 CSTS.RDY = 1
```

这个顺序不能乱。

## 5. Admin Queue 是第一版的核心

### 5.1 为什么先做 Admin Queue

因为第一版所有关键事情都靠 admin command：

1. `Identify Controller`
2. `Identify Namespace`
3. `Create Completion Queue`
4. `Create Submission Queue`

只要 admin queue 跑通，后面 IO queue 就只是“再造一对队列 + 换 opcode”。

### 5.2 Admin Queue 初始化顺序

看：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/pci.c:1690-1711`

流程：

```text
分配 admin SQ 内存
分配 admin CQ 内存
  ↓
aqa = ((cq_depth - 1) << 16) | (sq_depth - 1)
  ↓
写 NVME_REG_AQA
写 NVME_REG_ASQ
写 NVME_REG_ACQ
  ↓
enable controller
```

注意：

1. `AQA` 填的是 **零基深度**。
2. `ASQ/ACQ` 必须是物理地址。
3. SQE 大小固定 64B，CQE 大小固定 16B。

## 6. 命令格式你要亲手写出来

先看：

- `/home/inkbottle/桌面/linux-5.4.29/include/linux/nvme.h:672-976`
- `/home/inkbottle/桌面/linux-5.4.29/include/linux/nvme.h:1377-1390`

第一版只需要 5 个格式：

1. `nvme_common_command`
2. `nvme_rw_command`
3. `nvme_identify`
4. `nvme_create_cq`
5. `nvme_create_sq`
6. `nvme_completion`

你现在仓库里我已经给了对应骨架：

- `kernel/src/driver/nvme/mod.rs`

你后面要做的是：

1. 先保证这些结构 `#[repr(C)]`
2. 再加 `const_assert!` 或 `assert_eq!(size_of::<...>(), ...)`
3. 最后把字节序和字段填法一项项落实

第一版建议你在 `mod.rs` 里直接加：

```rust
assert_eq!(core::mem::size_of::<NvmeCommonCommand>(), 64);
assert_eq!(core::mem::size_of::<NvmeRwCommand>(), 64);
assert_eq!(core::mem::size_of::<NvmeIdentifyCommand>(), 64);
assert_eq!(core::mem::size_of::<NvmeCreateCompletionQueueCommand>(), 64);
assert_eq!(core::mem::size_of::<NvmeCreateSubmissionQueueCommand>(), 64);
assert_eq!(core::mem::size_of::<NvmeCompletionQueueEntry>(), 16);
```

## 7. 先把 completion polling 写对

第一版不要碰中断。

你要做的是：

```text
写一个 SQE 到 sq[tail]
  ↓
tail = (tail + 1) % depth
  ↓
敲 SQ doorbell
  ↓
轮询 cq[cq_head].status 的 phase bit
  ↓
phase 匹配 -> 说明 completion 有效
  ↓
读取 command_id / status / result
  ↓
cq_head = (cq_head + 1) % depth
如果 cq_head 回绕到 0，则 phase ^= 1
  ↓
敲 CQ head doorbell
```

这里最容易写错的点：

1. phase bit 回绕逻辑
2. SQ tail doorbell 和 CQ head doorbell 的索引
3. doorbell stride
4. command_id 和 completion 的对应关系

## 8. 第一版 I/O 不要碰 PRP list，只做单页 PRP

先看 Linux 的复杂版本：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/pci.c:758-843`

但你第一版不要全学，先砍成：

1. 只支持物理连续缓冲区
2. 只支持 `buffer_len <= 4096`
3. 只支持 `prp1 = page_base`
4. 如果数据跨页：
   - 允许 `prp2 = next_page_base`
   - 但先不要做 PRP list page

也就是说，第一版读写路径先压成：

```text
read/write request
  ↓
把 buffer 翻译成物理地址
  ↓
构造 NvmeRwCommand
  opcode = Read / Write
  nsid = 1
  slba = start_lba
  length = nlb - 1
  prp1 = first_page_phys
  prp2 = second_page_phys_or_0
  ↓
提交到唯一 IO SQ
  ↓
在唯一 IO CQ 上轮询完成
```

## 9. Create IO CQ / SQ 的顺序不能反

看：

- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/pci.c:1116-1167`
- `/home/inkbottle/桌面/linux-5.4.29/drivers/nvme/host/pci.c:1729-1764`

顺序是：

1. 先 Create CQ
2. 再 Create SQ

原因很简单：

- SQ 在创建时要引用 `cqid`
- 所以 CQ 必须先存在

第一版固定做一对：

1. `CQID = 1`
2. `SQID = 1`
3. `CQ Vector = 0` 或先不启用 IRQ
4. `Flags = PHYS_CONTIG`

## 10. 你在 BlueStarOS 里真正要写的顺序

按这个顺序干，不要跳步：

### Phase 1：只做枚举和寄存器读

1. 新建 `NvmePcieDeviceTarget`
2. 在 `PCIE_DEVICES` 里找到 class code = `0x010802`
3. 打印：
   - vendor/device
   - BDF
   - BAR0
   - CAP
   - VS
   - CSTS

通过标准：

1. 能稳定枚举出控制器
2. 能稳定读到 BAR0 寄存器

### Phase 2：只做控制器 enable

1. 关 `CC.EN`
2. 等 `CSTS.RDY = 0`
3. 写 `AQA/ASQ/ACQ`
4. 构造 `CC`
5. 开 `CC.EN`
6. 等 `CSTS.RDY = 1`

通过标准：

1. 控制器可重复 reset / enable
2. 不会卡死在 `RDY`

### Phase 3：只做 Admin Queue + Identify

1. 提交 `Identify Controller`
2. 提交 `Identify Namespace`
3. 解析：
   - `nn`
   - `nsze`
   - `flbas`
   - `lbaf[flbas].ds`

通过标准：

1. 能打印控制器基本信息
2. 能算出逻辑块大小
3. 能算出总块数

### Phase 4：只做 1 对 I/O Queue

1. Create CQ(1)
2. Create SQ(1)
3. 维护 `sq_tail/cq_head/cq_phase`

通过标准：

1. 能成功创建 IO queue
2. 不依赖中断，纯 polling 能拿到 completion

### Phase 5：只做 4KiB 读写

1. 构造 `NvmeRwCommand`
2. 先读 LBA 0
3. 再写一页测试块
4. 再读回校验

通过标准：

1. 单页读写正确
2. 数据一致

### Phase 6：接入 VFS

1. 写 `NvmeBlockDevice`
2. `impl BlockDevTrait for NvmeBlockDevice`
3. 在 probe 成功后：
   `register_global_block_device(Arc::new(Mutex::new(nvme_blk)))`

通过标准：

1. `RootFs::scan_and_build_vblock_device()` 能看到 NVMe 盘
2. 能自动生成 `/vda` 或 `/vdb`
3. 能挂 ext4 根分区

## 11. 现在就该怎么踢掉 virtio

不要一上来删。

正确顺序是：

1. 先把 NVMe 在 QEMU 上跑通
2. 先让它能读 Identify
3. 再让它能做单页读写
4. 再让它注册进 `GLOBAL_BLOCKS`
5. 最后再去掉 virtio-blk 的注册

当前 virtio-blk 的注册点：

- `kernel/src/arch/riscv64/driver/virtio_blk/block.rs:88-101`
- DTB probe 注册在：
  `kernel/src/arch/riscv64/driver/virtio_blk/block.rs:271-276`

因为 virtio-blk 是 DTB `Low` 优先级，而 PCIe host 扫描是 `High` 优先级：

- `kernel/src/driver/pcie/mod.rs:1070-1075`
- `kernel/src/arch/riscv64/driver/virtio_blk/block.rs:271-276`

所以你后面如果在 `pci_probe_callback()` 里直接完成 NVMe 注册，  
NVMe 会先进入 `GLOBAL_BLOCKS`，virtio 后进入。

这意味着：

1. 先调试阶段可以共存
2. 真正切换完成后，再把 virtio 的 `register_global_block_device()` 删掉最干净

## 12. 你应该先写哪个函数

按收益排序：

1. `probe_registered_pcie_nvme_devices()`
2. `NvmeController::probe_from_pcie_device()`
3. `NvmeController::disable_controller()`
4. `NvmeController::enable_controller()`
5. `NvmeController::submit_admin_command_polling()`
6. `NvmeController::identify_controller()`
7. `NvmeController::identify_namespace()`
8. `NvmeController::create_first_io_queue_pair()`
9. `NvmeController::read_logical_blocks_polling()`
10. `NvmeController::write_logical_blocks_polling()`
11. `NvmeBlockDevice: BlockDevTrait`

这 11 步的代码落点，我已经在：

- `kernel/src/driver/nvme/mod.rs`

里给你留了带 TODO 的骨架。

## 13. 第一个真正能证明你成功的里程碑

不是“枚举到 NVMe 设备”，也不是“读到 CAP/VS”。

第一个真正成功的里程碑是：

1. `Identify Namespace(1)` 成功
2. 解析出逻辑块大小
3. 用 I/O Queue 读回 LBA 0
4. 读到合法 GPT/MBR 签名

到这一步，驱动主干就已经通了。

后面把它包成 `BlockDevTrait`，只是系统接线，不再是协议攻坚。
