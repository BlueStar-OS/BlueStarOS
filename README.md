# BlueStarOS

![Rust](https://img.shields.io/badge/rust-latest-brightgreen)
![License](https://img.shields.io/badge/license-MIT-blue)
![Lines of Code](https://img.shields.io/badge/LoC-51K-orange)
![Status](https://img.shields.io/badge/status-active-success)

一个用纯 Rust 从零开发的操作系统内核，支持 RISC-V 64 和 AArch64 双架构。

## 特性

### 进程与调度

- 多进程管理: fork / exec / exit / wait / signal
- Stride 调度算法, 支持优先级权重
- 进程间信号 (SIGKILL / SIGINT / SIGTERM / SIGTSTP)
- 84 个系统调用, 兼容 musl libc 用户态程序

### 内存管理

- SV39 / AArch64 4K 页表, 用户/内核地址空间隔离
- Buddy System 物理页分配器
- 内核堆动态分配 (buddy_system_allocator)
- mmap 匿名映射

### 文件系统

- VFS 抽象层 + dentry/inode 缓存
- **ext4** (rsext4 crate, ~18K 行): extent tree, JBD2 事务日志, htree 目录索引, mkfs
- FAT32 / ramfs
- pipe, tty

### 设备驱动

- **PCIe**: 枚举, BAR 解析, MSI 中断
- **NVMe**: 块设备驱动 (替代 virtio-blk)
- **e1000 网卡**: RX/TX DMA 环形队列, 中断驱动
- virtio-blk / virtio-gpu
- DTB (设备树) 解析

### 网络协议栈 (进行中)

- L2: 以太网帧收发
- L3: IPv4 头解析/校验, ICMP Echo Request/Reply
- L4: UDP 头 + 伪头校验和
- ARP 协议 (请求/应答/缓存)

### 双架构支持

- `arch/riscv64`: QEMU virt 机器, PLIC 中断控制器
- `arch/aarch64`: QEMU virt + OrangePi 5 Plus (RK3588), GIC 中断控制器

## 目录结构

```text
kernel/
├── src/
│   ├── arch/              # 架构相关 (riscv64, aarch64)
│   │   ├── */driver/      # UART, PLIC/GIC, virtio
│   │   ├── */memory/      # 地址空间, 页表
│   │   ├── */task/        # 上下文切换
│   │   └── */trap/        # 陷阱处理
│   ├── driver/
│   │   ├── network/e1000/ # e1000 网卡驱动 + 协议栈
│   │   ├── nvme/          # NVMe 块设备驱动
│   │   ├── pcie/          # PCIe 枚举与 BAR
│   │   └── gpu/           # VGA 帧缓冲
│   ├── fs/
│   │   ├── vfs/           # VFS 抽象层 + 缓存
│   │   ├── fs_backend/    # ext4, FAT32, ramfs
│   │   └── component/     # pipe, tty
│   ├── memory/            # 物理/虚拟内存管理
│   ├── task/              # 进程控制块, 调度器
│   ├── syscall/           # 系统调用分发
│   └── sync/              # 同步原语
├── dependencies/
│   └── rsext4/            # 自研 ext4 文件系统 (~18K 行)
└── Cargo.toml
```

## 快速开始

**前置依赖**

- Rust nightly 工具链
- QEMU (7.0+)
- GNU Make

**构建与运行**

```bash
# 克隆
git clone https://github.com/Dirinkbottle/BlueStarOS.git
cd BlueStarOS/kernel

# RISC-V (默认)
make run LOG=TRACE

# AArch64
make run ARCH=aarch64 LOG=TRACE
```

## 开发计划

- [ ] 网络: socket syscall, 用户态 UDP/TCP
- [ ] 同步: 信号量, Mutex, condvar (WaitQueue 骨架已就位)
- [ ] SMP 多核支持
- [ ] 用户态 shell
- [ ] ext4 写入支持完善

## 许可证

MIT License. 详见 [LICENSE](LICENSE)。

## 联系方式

- 邮箱: <2909128143@qq.com>
- GitHub: [BlueStarOS](https://github.com/Dirinkbottle/BlueStarOS/)
- Issues: [提交问题或建议](https://github.com/Dirinkbottle/BlueStarOS/issues)
