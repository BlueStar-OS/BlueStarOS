# BlueStarOS

![Rust](https://img.shields.io/badge/Rust-nightly-orange)
![License](https://img.shields.io/badge/license-MIT-blue)
![Status](https://img.shields.io/badge/status-active-success)

BlueStarOS 是一个使用 Rust 编写的实验性操作系统内核，目标是从内核基础设施、文件系统、设备驱动到用户态运行环境逐步构建一套可理解、可扩展的完整系统。目前主要支持 **RISC-V 64** 与 **AArch64**。

> 项目仍处于快速开发阶段，接口、驱动和构建方式可能继续调整。

## 功能概览

### 进程与调度

- 多进程管理：`fork` / `exec` / `exit` / `wait`
- Stride 加权调度
- 基础 POSIX 风格信号支持
- Linux RISC-V/AArch64 风格系统调用接口
- 可运行静态链接的 musl libc 用户程序

### 内存管理

- RISC-V SV39 与 AArch64 4 KiB 页表
- 用户态 / 内核态地址空间隔离
- Buddy System 物理页分配
- 内核动态内存分配
- 用户地址空间与 `mmap` / `munmap` 支持

### 文件系统

- VFS 抽象层与 dentry/inode 缓存
- ext4：extent tree、JBD2、htree、mkfs 等实现位于 `kernel/dependencies/rsext4`
- FAT32 与 ramfs
- pipe、TTY 与设备文件
- GPT 分区扫描与 ext4 根文件系统挂载

### 设备驱动

- PCIe：枚举、BAR 解析、中断能力
- NVMe 块设备驱动
- Intel e1000 网卡驱动
- virtio-blk / virtio-gpu
- framebuffer / keyboard
- DTB（设备树）解析
- RISC-V PLIC 与 AArch64 GIC

### 网络协议栈

当前已包含基础网络路径：

- Ethernet
- ARP
- IPv4
- ICMP
- UDP

Socket 用户接口与更完整的传输层支持仍在继续完善。

## 支持平台

| 架构 | 平台 | 状态 |
| --- | --- | --- |
| RISC-V 64 | QEMU `virt` | 主要开发平台 |
| AArch64 | QEMU `virt` | 支持 |
| AArch64 | Orange Pi 5 Plus / RK3588 | 实验性支持 |

## 仓库结构

```text
BlueStarOS/
├── kernel/                 # 内核主体
│   ├── src/
│   │   ├── arch/           # riscv64 / aarch64 架构相关代码
│   │   ├── driver/         # PCIe、NVMe、网络、GPU 等驱动
│   │   ├── fs/             # VFS、文件系统与设备文件
│   │   ├── memory/         # 物理/虚拟内存管理
│   │   ├── syscall/        # 系统调用
│   │   ├── task/           # 进程、调度与信号
│   │   └── sync/           # 同步原语
│   ├── dependencies/
│   │   └── rsext4/         # ext4 实现
│   └── TestOS.mk           # 内核自测构建入口
├── user/                   # 用户态 Rust/C 程序与根文件系统镜像构建
├── test/                   # 独立内核测试用例
├── docs/                   # 架构、驱动和实现文档
└── Makefile                # 顶层构建入口
```

## 构建环境

推荐在 Linux 环境下构建。主要依赖：

- Rust nightly
- `rust-src`、`llvm-tools-preview`
- `cargo-binutils`
- GNU Make
- QEMU
- RISC-V musl 交叉编译器（构建 RISC-V C 用户程序时需要）
- `losetup`、`parted`、`mkfs.ext4`、`mkfs.vfat`（生成磁盘镜像时需要）

首次配置 Rust 目标与工具：

```bash
cd kernel
make env
```

## 快速开始

```bash
git clone https://github.com/BlueStar-OS/BlueStarOS.git
cd BlueStarOS/kernel

# RISC-V 64
make run LOG=INFO

# AArch64
make run ARCH=aarch64 LOG=INFO
```

RISC-V QEMU 默认配置包含 e1000 + TAP 网络设备；如果本机没有 `tap0`，请先配置 TAP，或根据需要调整 `kernel/Makefile` 中的 QEMU 网络参数。

只构建用户态与内核镜像可使用：

```bash
make build
```

## 测试

仓库内的 `test/` 与 `kernel/TestOS.mk` 用于维护独立测试用例。目前可以构建 syscall 测试与测试 rootfs：

```bash
cd kernel
make test_syscall
make testfs
```

测试 rootfs 的自动 QEMU 执行仍在完善中。

## 可选用户程序

`user/Makefile` 提供可选的 DoomGeneric 构建入口，但不会参与默认镜像构建。需要时显式指定源码目录：

```bash
cd user
make build-doom DOOM_DIR=/path/to/doomgeneric
make img
```

## 开发方向

- [ ] 完善 socket syscall 与用户态 UDP/TCP
- [ ] SMP 多核支持
- [ ] 完善系统调用兼容性与错误语义
- [ ] 扩展真实硬件平台驱动
- [ ] 持续完善测试与自动化验证

## 文档

实现笔记与驱动文档位于 [`docs/`](docs/)，包括 AArch64 中断/页表、e1000、NVMe、PCIe 与存储相关内容。

## 许可证

BlueStarOS 使用 [MIT License](LICENSE)。

## 参与开发

欢迎通过 [Issues](https://github.com/BlueStar-OS/BlueStarOS/issues) 提交问题、改进建议或讨论实现方案。
