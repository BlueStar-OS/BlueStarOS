# SD/MMC Driver Library 🦀

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024+-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-ARM64-green.svg)](#)


## 📋 Table of Contents
- [Project Overview](#project-overview)
- [Features](#features)
- [Quick Start](#quick-start)
  - [Requirements](#requirements)
  - [Installation](#installation)
  - [Basic Usage](#basic-usage)
- [Project Structure](#project-structure)
- [API Documentation](#api-documentation)
  - [Core Structures](#core-structures)
  - [Main Interfaces](#main-interfaces)
  - [Supported Transfer Modes](#supported-transfer-modes)
- [Usage Examples](#usage-examples)
- [Test Results](#test-results)
  - [Running Tests](#running-tests)
  - [Test Functionality Description](#test-functionality-description)
- [Board Support](#board-support)
- [Troubleshooting](#troubleshooting)
  - [Common Issues](#common-issues)
  - [Debugging Tips](#debugging-tips)
- [License](#license)

## 📖 Project Overview

The SD/MMC Driver Library is a Rust SD/MMC controller driver library designed specifically for ARM64 platforms, supporting eMMC, SD, and SDIO devices. This library provides comprehensive storage controller functionality, including command sending, clock configuration, and block read/write operations.

This project uses a `no_std` design, making it fully suitable for bare-metal and embedded environments, with specific optimizations for the U-Boot bootloader environment. Through type-safe register access, it ensures the reliability and safety of hardware operations.

## ✨ Features

- 🧠 **Complete MMC/eMMC Support**: Supports eMMC 4.x/5.x standards, including high-speed mode, DDR mode, HS200, and HS400 modes
- 💳 **SD/SDIO Support**: Supports SD 1.0/2.0 standards and SDIO devices
- 🚀 **Multiple Data Transfer Modes**: Supports both PIO and DMA data transfer modes
- 🏔 **Rockchip Platform Optimization**: Specifically optimized for the RK3568 platform, supporting DWCMSHC controller
- 🔒 **Type-Safe Register Access**: Provides type-safe hardware register operations based on direct memory access
- 📦 **no_std Compatible**: Completely independent of the standard library, suitable for bare-metal and embedded environments
- ⚡ **ARM64 Architecture Optimization**: Specifically optimized for ARM64 platforms
- 🖥 **U-Boot Environment Support**: Provides stable and reliable storage access functionality in the U-Boot bootloader environment

## 🚀 Quick Start

### 🛠 Requirements

- Rust 2024 Edition
- ARM64 development environment
- Rockchip RK3568 hardware platform with U-Boot support
- ostool tool (for testing)

### 📦 Installation

1. Install the `ostool` dependency:

```bash
cargo install ostool
```

2. Add the project to `Cargo.toml`:

```toml
[dependencies]
sdmmc = "0.1.0"
```

### 📝 Basic Usage

```rust
use sdmmc::emmc::EMmcHost;
use core::ptr::NonNull;

// Create EMMC controller instance
let emmc_addr = 0xfe2e0000; // RK3568 EMMC controller base address
let mut emmc = EMmcHost::new(emmc_addr);

// Initialize controller and storage card
match emmc.init() {
    Ok(_) => {
        println!("EMMC initialized successfully");
        
        // Read storage card information
        match emmc.get_card_info() {
            Ok(card_info) => {
                println!("Card type: {:?}", card_info.card_type);
                println!("Capacity: {} MB", card_info.capacity_bytes / (1024 * 1024));
            }
            Err(e) => println!("Failed to get card info: {:?}", e),
        }
        
        // Read data block
        let mut buffer: [u8; 512] = [0; 512];
        match emmc.read_blocks(0, 1, &mut buffer) {
            Ok(_) => println!("Read data block successfully"),
            Err(e) => println!("Failed to read data block: {:?}", e),
        }
    }
    Err(e) => println!("EMMC initialization failed: {:?}", e),
}
```

## 📁 Project Structure

```
src/
├── lib.rs              # Main entry and core functionality
├── err.rs              # Error type definitions
└── emmc/
    ├── mod.rs          # EMMC module main file
    ├── cmd.rs          # Command sending and response handling
    ├── block.rs        # Block read/write operations
    ├── regs.rs         # Register access interface
    ├── constant.rs     # Hardware constant definitions
    ├── clock.rs        # Clock control interface
    ├── rockchip.rs     # Rockchip platform-specific implementation
    ├── config.rs       # Platform configuration
    ├── aux.rs          # Auxiliary functions
    └── info.rs         # Card information handling

tests/
└── test.rs             # Integration tests, including EMMC functionality tests
```

## 📚 API Documentation

### 🧱 Core Structures

| Structure | Description |
|-----------|-------------|
| **`EMmcHost`** | Main EMMC controller interface structure, providing all storage control functionality |
| **`EMmcCard`** | Storage card information structure, containing detailed card information |

### 🔧 Main Interfaces

#### 🎛 EMMC Controller Management

| Method | Description |
|--------|-------------|
| `EMmcHost::new(addr)` | Create new EMMC controller instance |
| `EMmcHost::init()` | Initialize EMMC controller and storage card |
| `EMmcHost::get_card_info()` | Get storage card information |
| `EMmcHost::get_status()` | Get controller status |

#### 💾 Data Read/Write Operations

| Method | Description |
|--------|-------------|
| `EMmcHost::read_blocks(block_id, blocks, buffer)` | Read data blocks |
| `EMmcHost::write_blocks(block_id, blocks, buffer)` | Write data blocks |

#### ⏱ Clock and Bus Control

| Method | Description |
|--------|-------------|
| `EMmcHost::mmc_set_clock(freq)` | Set clock frequency |
| `EMmcHost::mmc_set_bus_width(width)` | Set bus width |
| `EMmcHost::mmc_set_timing(timing)` | Set timing mode |

### 🔄 Supported Transfer Modes

| Mode | Description |
|------|-------------|
| **PIO Mode** | Enabled by default, suitable for small data transfers |
| **DMA Mode** | Enabled via `dma` feature, suitable for large data transfers |

## 💡 Usage Examples

### 🔧 EMMC Initialization Example

```rust
use sdmmc::emmc::EMmcHost;
use core::ptr::NonNull;

fn init_emmc_controller(emmc_addr: usize) -> Result<(), &'static str> {
    // Create EMMC controller instance
    let mut emmc = EMmcHost::new(emmc_addr);
    
    // Initialize controller
    match emmc.init() {
        Ok(_) => {
            println!("EMMC controller initialized successfully");
            
            // Get card information
            match emmc.get_card_info() {
                Ok(card_info) => {
                    println!("Card type: {:?}", card_info.card_type);
                    println!("Manufacturer ID: 0x{:02X}", card_info.manufacturer_id);
                    println!("Capacity: {} MB", card_info.capacity_bytes / (1024 * 1024));
                    println!("Block size: {} bytes", card_info.block_size);
                }
                Err(e) => {
                    println!("Failed to get card information: {:?}", e);
                    return Err("Failed to get card information");
                }
            }
            
            Ok(())
        }
        Err(e) => {
            println!("EMMC controller initialization failed: {:?}", e);
            Err("Controller initialization failed")
        }
    }
}
```

### 💾 Data Read/Write Example

```rust
use sdmmc::emmc::EMmcHost;

fn read_write_test(emmc: &mut EMmcHost) -> Result<(), &'static str> {
    // Read first data block
    let mut read_buffer: [u8; 512] = [0; 512];
    match emmc.read_blocks(0, 1, &mut read_buffer) {
        Ok(_) => {
            println!("Read data block successfully");
            println!("First 16 bytes: {:02X?}", &read_buffer[0..16]);
        }
        Err(e) => {
            println!("Failed to read data block: {:?}", e);
            return Err("Read failed");
        }
    }
    
    // Write test data to third data block
    let mut write_buffer: [u8; 512] = [0; 512];
    // Fill test data
    for i in 0..512 {
        write_buffer[i] = (i % 256) as u8;
    }
    
    match emmc.write_blocks(2, 1, &write_buffer) {
        Ok(_) => println!("Write data block successfully"),
        Err(e) => {
            println!("Failed to write data block: {:?}", e);
            return Err("Write failed");
        }
    }
    
    // Read back for verification
    let mut verify_buffer: [u8; 512] = [0; 512];
    match emmc.read_blocks(2, 1, &mut verify_buffer) {
        Ok(_) => {
            // Verify data consistency
            let mut data_match = true;
            for i in 0..512 {
                if write_buffer[i] != verify_buffer[i] {
                    data_match = false;
                    break;
                }
            }
            
            if data_match {
                println!("Data verification successful");
            } else {
                println!("Data verification failed");
                return Err("Data verification failed");
            }
        }
        Err(e) => {
            println!("Verification read failed: {:?}", e);
            return Err("Verification failed");
        }
    }
    
    Ok(())
}
```

### 🎯 Complete Usage Example

```rust
use sdmmc::emmc::EMmcHost;
use core::ptr::NonNull;

fn main() -> Result<(), &'static str> {
    // EMMC controller base address (RK3568)
    let emmc_addr = 0xfe2e0000;
    
    // Create controller instance
    let mut emmc = EMmcHost::new(emmc_addr);
    
    // Initialize controller
    println!("Initializing EMMC controller...");
    if let Err(e) = emmc.init() {
        println!("EMMC controller initialization failed: {:?}", e);
        return Err("Initialization failed");
    }
    
    // Get card information
    println!("Getting storage card information...");
    match emmc.get_card_info() {
        Ok(card_info) => {
            println!("Card type: {:?}", card_info.card_type);
            println!("Manufacturer ID: 0x{:02X}", card_info.manufacturer_id);
            println!("Capacity: {} MB", card_info.capacity_bytes / (1024 * 1024));
            println!("Block size: {} bytes", card_info.block_size);
        }
        Err(e) => {
            println!("Failed to get card information: {:?}", e);
            return Err("Failed to get card information");
        }
    }
    
    // Execute read/write test
    println!("Executing read/write test...");
    if let Err(e) = read_write_test(&mut emmc) {
        println!("Read/write test failed: {}", e);
        return Err(e);
    }
    
    println!("All tests completed");
    Ok(())
}

fn read_write_test(emmc: &mut EMmcHost) -> Result<(), &'static str> {
    // Read first data block
    let mut read_buffer: [u8; 512] = [0; 512];
    match emmc.read_blocks(0, 1, &mut read_buffer) {
        Ok(_) => {
            println!("Read data block successfully");
            println!("First 16 bytes: {:02X?}", &read_buffer[0..16]);
        }
        Err(e) => {
            println!("Failed to read data block: {:?}", e);
            return Err("Read failed");
        }
    }
    
    // Write test data to third data block
    let mut write_buffer: [u8; 512] = [0; 512];
    // Fill test data
    for i in 0..512 {
        write_buffer[i] = (i % 256) as u8;
    }
    
    match emmc.write_blocks(2, 1, &write_buffer) {
        Ok(_) => println!("Write data block successfully"),
        Err(e) => {
            println!("Failed to write data block: {:?}", e);
            return Err("Write failed");
        }
    }
    
    // Read back for verification
    let mut verify_buffer: [u8; 512] = [0; 512];
    match emmc.read_blocks(2, 1, &mut verify_buffer) {
        Ok(_) => {
            // Verify data consistency
            let mut data_match = true;
            for i in 0..512 {
                if write_buffer[i] != verify_buffer[i] {
                    data_match = false;
                    break;
                }
            }
            
            if data_match {
                println!("Data verification successful");
            } else {
                println!("Data verification failed");
                return Err("Data verification failed");
            }
        }
        Err(e) => {
            println!("Verification read failed: {:?}", e);
            return Err("Verification failed");
        }
    }
    
    Ok(())
}
```

## 🧪 Test Results

### ▶️ Running Tests

#### 🔌 Hardware Testing with U-Boot Environment

```bash
# Board testing with u-boot
make uboot
```

### Test Output Example

<details>
<summary>Click to view test results</summary>

```
     _____                                         __
    / ___/ ____   ____ _ _____ _____ ___   ____ _ / /
    \__ \ / __ \ / __ `// ___// ___// _ \ / __ `// / 
   ___/ // /_/ // /_/ // /   / /   /  __// /_/ // /  
  /____// .___/ \__,_//_/   /_/    \___/ \__,_//_/   
/_/                                           

Version                       : 0.12.2
Platfrom                      : RK3588 OPi 5 Plus
Start CPU                     : 0x0
FDT                           : 0xffff900000f29000
🐛 0.000ns    [sparreal_kernel::driver:16] add registers
🐛 0.000ns    [rdrive::probe::fdt:168] Probe [interrupt-controller@fe600000]->[GICv3]
🐛 0.000ns    [somehal::arch::mem::mmu:181] Map `iomap       `: RW- | [0xffff9000fe600000, 0xffff9000fe610000) -> [0xfe600000, 0xfe610000)
🐛 0.000ns    [somehal::arch::mem::mmu:181] Map `iomap       `: RW- | [0xffff9000fe680000, 0xffff9000fe780000) -> [0xfe680000, 0xfe780000)
🐛 0.000ns    [rdrive::probe::fdt:168] Probe [timer]->[ARMv8 Timer]
🐛 0.000ns    [sparreal_rt::arch::timer:78] ARMv8 Timer IRQ: IrqConfig { irq: 0x1e, trigger: LevelHigh, is_private: true }
🐛 0.000ns    [rdrive::probe::fdt:168] Probe [psci]->[ARM PSCI]
🐛 0.000ns    [spar:power:76] PCSI [Smc]
🐛 0.000ns    [sparreal_kernel::irq:39] [GICv3](405) open
🔍 0.000ns    [arm_gic_driver::version::v3:342] Initializing GICv3 Distributor@0xffff9000fe600000, security state: NonSecure...
🔍 0.000ns    [arm_gic_driver::version::v3:356] GICv3 Distributor disabled
🔍 0.000ns    [arm_gic_driver::version::v3:865] CPU interface initialization for CPU: 0x0
🔍 0.000ns    [arm_gic_driver::version::v3:921] CPU interface initialized successfully
🐛 0.000ns    [sparreal_kernel::irq:64] [GICv3](405) init cpu: CPUHardId(0)
🐛 0.000ns    [sparreal_rt::arch::timer:30] ARMv8 Timer: Enabled
🐛 17.681s    [sparreal_kernel::irq:136] Enable irq 0x1e on chip 405
🐛 17.681s    [sparreal_kernel::hal_al::run:33] Driver initialized
🐛 18.304s    [rdrive:132] probe pci devices
begin test
Run test: test_platform
💡 18.358s    [test::tests:243] Found node: mmc@fe2e0000
💡 18.359s    [test::tests:248💡 18.390s    [test::tests:243] Found node: clock-controller@fd7c0000
💡 18.390s    [teests:48] clk ptr: 0xffff9000fd7c0000
💡 18.395s    [test::tests:53] emmc addr: 0xffff9000fe2e0000
💡 18.396s    [test::tests:54] clk addr: 0xffff9000fd7c0000
💡 18.397s    [sdmmc::emmc:74] EMMC Controller created: EMMC Controller { base_addr: 0xffff9000fe2e0000, card: None, caps: 0x226dc881, clock_base: 200000000 }
💡 18.398s    [sdmmc::emmc:91] Init EMMC Controller
🐛 18.399s    [sdmmc::emmc:100] Card inserted: true
💡 18.399s    [sdmmc::emmc:105] EMMC Version: 0x5
💡 18.400s    [sdmmc::emmc:108] EMMC Capabilities 1: 0b100010011011011100100010000001
💡 18.401s    [sdmmc::emmc:114] EMMC Capabilities 2: 0b1000000000000000000000000111
💡 18.402s    [sdmmc::emmc:162] voltage range: 0x60000, 0x12
💡 18.402s    [sdmmc::emmc::rockchip:145] EMMC Power Control: 0xd
🐛 18.413s    [sdmmc::emmc:974] Bus width set to 1
🐛 18.414s    [sdmmc::emmc::rockchip:318] card_clock: 0, bus_width: 1, timing: 0
💡 18.415s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x0
🐛 18.415s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.416s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.417s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x0
🐛 18.417s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x0
🐛 18.418s    [sdmmc::emmc::rockchip:318] card_clock: 400000, bus_width: 1, timing: 0
🐛 18.419s    [rk3588_clk:111] Setting clk_id 314 to rate 400000
🐛 18.420s    [rk3588_clk:152] CCLK_EMMC: src_clk 2, div 60, new_value 0xbb00, final_value 0xff00bb00
🐛 18.421s    [rk3588_clk:73] Getting clk_id 314
💡 18.421s    [sdmmc::emmc::rockchip:32] input_clk: 400000
💡 18.422s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.423s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x0
🐛 18.423s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.424s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x2
🐛 18.425s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.426s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.426s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x0
🐛 18.427s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x0
🐛 18.428s    [sdmmc::emmc::rockchip:318] card_clock: 400000, bus_width: 1, timing: 0
🐛 18.428s    [rk3588_clk:111] Setting clk_id 314 to rate 400000
🐛 18.429s    [rk3588_clk:152] CCLK_EMMC: src_clk 2, div 60, new_value 0xbb00, final_value 0xff00bb00
🐛 18.430s    [rk3588_clk:73] Getting clk_id 314
💡 18.431s    [sdmmc::emmc::rockchip:32] input_clk: 400000
💡 18.431s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.432s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x0
🐛 18.433s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.434s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x2
🐛 18.434s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.435s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.436s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x0
🐛 18.436s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x0
💡 18.437s    [sdmmc::emmc:226] eMMC initialization started
🔍 18.438s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x0, arg=0x0, resp_type=0x0, command=0x0
🔍 18.439s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.440s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.440s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.453s    [sdmmc::emmc::cmd:416] eMMC reset complete
🔍 18.454s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x0, resp_type=0x1, command=0x102
🔍 18.455s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.455s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.456s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.469s    [sdmmc::emmc::cmd:431] eMMC first CMD1 response (no args): 0xff8080
🔍 18.470s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x40060000, resp_type=0x1, command=0x102
🔍 18.471s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.472s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.472s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.475s    [sdmmc::emmc::cmd:453] CMD1 response raw: 0xff8080
💡 18.476s    [sdmmc::emmc::cmd:454] eMMC CMD1 response: 0xff8080
🔍 18.477s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x40060000, resp_type=0x1, command=0x102
🔍 18.479s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.479s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.480s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.483s    [sdmmc::emmc::cmd:453] CMD1 response raw: 0xff8080
💡 18.484s    [sdmmc::emmc::cmd:454] eMMC CMD1 response: 0xff8080
🔍 18.485s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x40060000, resp_type=0x1, command=0x102
🔍 18.486s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.487s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.488s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.491s    [sdmmc::emmc::cmd:453] CMD1 response raw: 0xff8080
💡 18.491s    [sdmmc::emmc::cmd:454] eMMC CMD1 response: 0xff8080
🔍 18.493s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x40060000, resp_type=0x1, command=0x102
🔍 18.494s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.495s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.496s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.498s    [sdmmc::emmc::cmd:453] CMD1 response raw: 0xff8080
💡 18.499s    [sdmmc::emmc::cmd:454] eMMC CMD1 response: 0xff8080
🔍 18.501s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x40060000, resp_type=0x1, command=0x102
🔍 18.502s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.503s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.503s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.506s    [sdmmc::emmc::cmd:453] CMD1 response raw: 0xff8080
💡 18.507s    [sdmmc::emmc::cmd:454] eMMC CMD1 response: 0xff8080
🔍 18.508s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x1, arg=0x40060000, resp_type=0x1, command=0x102
🔍 18.510s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.510s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.511s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.514s    [sdmmc::emmc::cmd:453] CMD1 response raw: 0xc0ff8080
💡 18.514s    [sdmmc::emmc::cmd:454] eMMC CMD1 response: 0xc0ff8080
💡 18.515s    [sdmmc::emmc::cmd:478] eMMC initialization status: true
🐛 18.517s    [sdmmc::emmc::cmd:486] Clock control before CMD2: 0x7, stable: true
🔍 18.518s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x2, arg=0x0, resp_type=0x7, command=0x209
🔍 18.519s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.520s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.520s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.523s    [sdmmc::emmc::cmd:69] eMMC response: 0x45010044 0x56343033 0x3201bb29 0x7a017c00
🔍 18.524s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x3, arg=0x10000, resp_type=0x15, command=0x31a
🔍 18.525s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.526s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.527s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🔍 18.529s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x9, arg=0x10000, resp_type=0x7, command=0x909
🔍 18.530s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.531s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.532s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
💡 18.535s    [sdmmc::emmc::cmd:69] eMMC response: 0xd00f0032 0x8f5903ff 0xffffffef 0x8a404000
🐛 18.536s    [sdmmc::emmc:256] eMMC CSD version: 4
🔍 18.536s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x7, arg=0x10000, resp_type=0x15, command=0x71a
🔍 18.537s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.538s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.539s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🐛 18.541s    [sdmmc::emmc:327] cmd7: 0x700
🔍 18.542s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x6, arg=0x3b90100, resp_type=0x1d, command=0x61b
🔍 18.543s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.544s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.545s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🐛 18.547s    [sdmmc::emmc:1010] cmd6 0x800
🔍 18.548s    [sdmmc::emmc::cmd:244] Sending command: opcode=0xd, arg=0x10000, resp_type=0x15, command=0xd1a
🔍 18.549s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.550s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.550s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🔍 18.553s    [sdmmc::emmc::cmd:583] cmd_d 0x900
🐛 18.554s    [sdmmc::emmc::rockchip:318] card_clock: 400000, bus_width: 1, timing: 1
🐛 18.555s    [rk3588_clk:111] Setting clk_id 314 to rate 400000
🐛 18.555s    [rk3588_clk:152] CCLK_EMMC: src_clk 2, div 60, new_value 0xbb00, final_value 0xff00bb00
🐛 18.556s    [rk3588_clk:73] Getting clk_id 314
💡 18.557s    [sdmmc::emmc::rockchip:32] input_clk: 400000
💡 18.558s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.558s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x0
🐛 18.559s    [sdmmc::emmc::rockchip:106] EMMC Clocckchip:106] EMMC Clock Control: 0x7
💡 18.561s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.562s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x4
🐛 18.563s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x2
🐛 18.563s    [sdmmc::emmc::rockchip:318] card_clock: 52000000, bus_width: 1, timing: 1
🐛 18.564s    [rk3588_clk:111] Setting clk_id 314 to rate 52000000
🐛 18.565s    [rk3588_clk:152] CCLK_EMMC: src_clk 1, div 23, new_value 0x5600, final_value 0xff005600
🐛 18.566s    [rk3588_clk:73] Getting clk_id 314
💡 18.567s    [sdmmc::emmc::rockchip:32] input_clk: 65217391
💡 18.567s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.568s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x1
🐛 18.569s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x107
💡 18.569s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x2
🐛 18.570s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.571s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.571s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x4
🐛 18.572s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x2
🔍 18.573s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x8, arg=0x0, resp_type=0x15, command=0x83a
🔍 18.574s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.575s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.575s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🔍 18.576s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
🔍 18.577s    [sdmmc::emmc:354] EXT_CSD: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 3, 0, 144, 23, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 146, 4, 0, 7, 0, 0, 2, 0, 0, 21, 31, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 13, 0, 0, 0, 0, 8, 0, 2, 0, 87, 31, 10, 3, 221, 221, 0, 0, 0, 10, 10, 10, 10, 10, 10, 1, 0, 224, 163, 3, 23, 19, 23, 7, 7, 16, 1, 3, 1, 8, 32, 0, 7, 166, 166, 85, 3, 0, 0, 0, 0, 221, 221, 0, 1, 255, 0, 0, 0, 0, 1, 25, 25, 0, 16, 0, 0, 221, 82, 67, 51, 48, 66, 48, 48, 55, 81, 80, 8, 8, 8, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 31, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 16, 0, 3, 3, 0, 5, 3, 3, 1, 63, 63, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0]
🐛 18.591s    [sdmmc::emmc:412] Boot partition size: 0x400000
🐛 18.591s    [sdmmc::emmc:413] RPMB partition size: 0x1000000
🐛 18.592s    [sdmmc::emmc:434] GP partition sizes: [0, 0, 0, 0]
🔍 18.593s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x8, arg=0x0, resp_type=0x15, command=0x83a
🔍 18.594s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.595s    [sdmmc::emmc::cmd:263] Response Status: 0b100001
🔍 18.595s    [sdmmc::emmc::cmd:288] Command completed: status=0b100001
🔍 18.596s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
🔍 18.597s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x8, arg=0x0, resp_type=0x15, command=0x83a
🔍 18.598s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.599s    [sdmmc::emmc::cmd:263] Response Status: 0b100001
🔍 18.599s    [sdmmc::emmc::cmd:288] Command completed: status=0b100001
🔍 18.600s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
🔍 18.601s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x6, arg=0x3b70200, resp_type=0x1d, command=0x61b
🔍 18.602s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.603s    [sdmmc::emmc::cmd:263] Response Status: 0b11
🔍 18.604s    [sdmmc::emmc::cmd:288] Command completed: status=0b11
🐛 18.604s    [sdmmc::emmc:1010] cmd6 0x800
🔍 18.605s    [sdmmc::emmc::cmd:244] Sending command: opcode=0xd, arg=0x10000, resp_type=0x15, command=0xd1a
🔍 18.606s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.607s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.608s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🔍 18.608s    [sdmmc::emmc::cmd:583] cmd_d 0x900
🐛 18.609s    [sdmmc::emmc:974] Bus width set to 8
🐛 18.609s    [sdmmc::emmc::rockchip:318] card_clock: 52000000, bus_width: 8, timing: 1
🐛 18.610s    [rk3588_clk:111] Setting clk_id 314 to rate 52000000
🐛 18.611s    [rk3588_clk:152] CCLK_EMMC: src_clk 1, div 23, new_value 0x5600, final_value 0xff005600
🐛 18.612s    [rk3588_clk:73] Getting clk_id 314
💡 18.613s    [sdmmc::emmc::rockchip:32] input_clk: 65217391
💡 18.613s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.614s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x1
🐛 18.615s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x107
💡 18.616s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x2
🐛 18.616s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.617s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.618s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x24
🐛 18.618s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x2
🔍 18.619s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x8, arg=0x0, resp_type=0x15, command=0x83a
🔍 18.620s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.621s    [sdmmc::emmc::cmd:263] Response Status: 0b1
🔍 18.622s    [sdmmc::emmc::cmd:288] Command completed: status=0b1
🔍 18.622s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
🔍 18.623s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x6, arg=0x3b90200, resp_type=0x1d, command=0x61b
🔍 18.624s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.625s    [sdmmc::emmc::cmd:263] Response Status: 0b11
🔍 18.626s    [sdmmc::emmc::cmd:288] Command completed: status=0b11
🐛 18.626s    [sdmmc::emmc:1010] cmd6 0x800
🐛 18.628s    [sdmmc::emmc::rockchip:318] card_clock: 52000000, bus_width: 8, timing: 9
🐛 18.629s    [rk3588_clk:111] Setting clk_id 314 to rate 52000000
🐛 18.630s    [rk3588_clk:152] CCLK_EMMC: src_clk 1, div 23, new_value 0x5600, final_value 0xff005600
🐛 18.631s    [rk3588_clk:73] Getting clk_id 314
💡 18.631s    [sdmmc::emmc::rockchip:32] input_clk: 65217391
💡 18.632s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.633s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x1
🐛 18.633s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x107
💡 18.634s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x2
🐛 18.635s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.636s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.636s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x24
💡 18.637s    [sdmmc::emmc::rockchip:145] EMMC Power Control: 0xb
🐛 18.648s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x1b
🐛 18.648s    [sdmmc::emmc::rockchip:318] card_clock: 200000000, bus_width: 8, timing: 9
🐛 18.649s    [rk3588_clk:111] Setting clk_id 314 to rate 200000000
🐛 18.650s    [rk3588_clk:152] CCLK_EMMC: src_clk 1, div 6, new_value 0x4500, final_value 0xff004500
🐛 18.651s    [rk3588_clk:73] Getting clk_id 314
💡 18.652s    [sdmmc::emmc::rockchip:32] input_clk: 250000000
💡 18.652s    [sdmmc::emmc::rockchip:42] EMMC Clock Mul: 0
💡 18.653s    [sdmmc::emmc::rockchip:78] EMMC Clock Divisor: 0x1
🐛 18.654s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x107
💡 18.654s    [sdmmc::emmc::rockchip:163] EMMC Clock Control: 0x2
🐛 18.657s    [sdmmc::emmc::rockchip:106] EMMC Clock Control: 0x7
💡 18.658s    [sdmmc::emmc::rockchip:275] Clock 0x7
🐛 18.658s    [sdmmc::emmc::rockchip:353] EMMC Host Control 1: 0x24
💡 18.659s    [sdmmc::emmc::rockchip:145] EMMC Power Control: 0xb
🐛 18.670s    [sdmmc::emmc::rockchip:307] EMMC Host Control 2: 0x1b
🔍 18.671s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.672s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.673s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.673s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.674s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.675s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.676s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.677s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.677s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.678s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.679s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.680s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.681s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.681s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.683s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.683s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.684s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.685s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.686s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.687s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.687s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.688s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.689s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.690s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.691s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.692s    [sdmmc::emmc::cmd:24resp_type=0x15, command=0x153a
🔍 18.693s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.693s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.694s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.695s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.696s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.697s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.697s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.698s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.699s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.700s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.701s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.702s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.703s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.703s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.704s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.705s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.706s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.707s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.707s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.708s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.709s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.710s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.711s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.712s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.713s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.713s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.714s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.715s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.716s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.717s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.717s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.718s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.719s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.720s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.721s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.722s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.723s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.723s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.724s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 1    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.726s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.727s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.727s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.728s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.729s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.730s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.731s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.732s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.733s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.733s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.734s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.735s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.736s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.737s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.738s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.738s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.739s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.740s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.741s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.742s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.743s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.743s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.744s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.745s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.746s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.747s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.748s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.748s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.749s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.750s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.751s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.752s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.753s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.754s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.754s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.755s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.756s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.757s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.758s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.758s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.759s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.760s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.761s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.762s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.763s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.764s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.764s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.765s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.766s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.767s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.768s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.768s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.769s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.770s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.771s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
🔍 18.772s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x15, arg=0x0, resp_type=0x15, command=0x153a
🔍 18.773s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.774s    [sdmmc::emmc::cmd:263] Response Status: 0b100000
🔍 18.774s    [sdmmc::emmc::cmd:nse Status: 0b100000
🔍 18.778s    [sdmmc::emmc::cmd:288] Command completed: status=0b100000
ully
SD card initialization successful!
Card type: MmcHc
Manufacturer ID: 0x45
Capacity: 0 MB
Block size: 512 bytes
Attempting to read first block...
🔍 18.780s    [sdmmc::emmc::block:365] pio read_blocks: block_id = 5034498, blocks = 1
🔍 18.781s    [sdmmc::emmc::block:383] Reading 1 blocks starting at address: 0x4cd202
🔍 18.782s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x11, arg=0x4cd202, resp_type=0x15, command=0x113a
🔍 18.783s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.784s    [sdmmc::emmc::cmd:263] Response Status: 0b100001
🔍 18.785s    [sdmmc::emmc::cmd:288] Command completed: status=0b100001
🔍 18.786s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
Successfully read first block!
First 16 bytes of first block: [40, E2, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 8F, D2, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, DB, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, E0, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, C0, EC, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, E9, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, EE, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, E4, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, C0, DE, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, F0, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, DD, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, E7, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, A9, D5, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, 5B, D7, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, 50, D6, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, 4E, D6, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 60, 4F, D6, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, CE, CD, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, 48, DF, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 8E, D2, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 60, D6, CD, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 90, D2, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, A0, 09, DD, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 80, B9, E1, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, EB, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 60, DD, E0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 20, D1, CD, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, E0, 7E, E2, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 20, A8, D5, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 40, D7, CD, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 91, D2, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, C0, E5, D0, 01, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00, 00]
Testing write and read back...
🔍 18.804s    [sdmmc::emmc::block:417] pio write_blocks: block_id = 3, blocks = 1
🔍 18.805s    [sdmmc::emmc::block:439] Writing 1 blocks starting at address: 0x3
🔍 18.806s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x18, arg=0x3, resp_type=0x15, command=0x183a
🔍 18.807s    [sdmmc::emmc::cmd:263] Response Status: 0b10000
🔍 18.808s    [sdmmc::emmc::cmd:263] Response Status: 0b10001
🔍 18.808s    [sdmmc::emmc::cmd:288] Command completed: status=0b10001
🔍 18.809s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
Successfully wrote to block 3!
🔍 18.811s    [sdmmc::emmc::block:365] pio read_blocks: block_id = 3, blocks = 1
🔍 18.812s    [sdmmc::emmc::block:383] Reading 1 blocks starting at address: 0x3
🔍 18.813s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x11, arg=0x3, resp_type=0x15, command=0x113a
🔍 18.814s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.815s    [sdmmc::emmc::cmd:263] Response Status: 0b100001
🔍 18.816s    [sdmmc::emmc::cmd:288] Command completed: status=0b100001
🔍 18.816s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
Successfully read back block 3!
First 16 bytes of read block: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
Data verification successful: written and read data match perfectly!
Testing multi-block read...
🔍 18.831s    [sdmmc::emmc::block:365] pio read_blocks: block_id = 200, blocks = 4
🔍 18.832s    [sdmmc::emmc::block:383] Reading 4 blocks starting at address: 0xc8
🔍 18.833s    [sdmmc::emmc::cmd:244] Sending command: opcode=0x12, arg=0xc8, resp_type=0x15, command=0x123a
🔍 18.834s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.835s    [sdmmc::emmc::cmd:263] Response Status: 0b100001
🔍 18.835s    [sdmmc::emmc::cmd:288] Command completed: status=0b100001
🔍 18.836s    [sdmmc::emmc::cmd:339] Data transfer: cmd.data_present=true
🔍 18.837s    [sdmmc::emmc::cmd:244] Sending command: opcode=0xc, arg=0x0, resp_type=0x1d, command=0xc1b
🔍 18.838s    [sdmmc::emmc::cmd:263] Response Status: 0b0
🔍 18.839s    [sdmmc::emmc::cmd:263] Response Status: 0b11
🔍 18.840s    [sdmmc::emmc::cmd:288] Command completed: status=0b11
Successfully read 4 blocks starting at block address 200!
First 16 bytes of first block: [A0, 2F, 00, B9, A1, 8B, 0D, A9, A0, 07, 42, A9, A0, 07, 04, A9]
First 16 bytes of last block: [B5, 01, BD, 01, C6, 01, CE, 01, D6, 01, DE, 01, E7, 01, EF, 01]
SD card test complete
💡 18.843s    [test::tests:58] test uboot
test test_platform passed
All tests passed
```

</details>

### 📋 Test Functionality Description

The test program performs the following operations:

1. **Device Tree Parsing**: Find EMMC controller hardware node addresses from the device tree
2. **EMMC Controller Initialization**: Initialize the DWCMSHC EMMC controller
3. **Storage Card Detection**: Detect and initialize the connected eMMC storage card
4. **Basic Read/Write Tests**:
   - Read storage card information
   - Read data blocks
   - Write data blocks and verify
   - Multi-block read tests
5. **Data Consistency Verification**: Verify that written and read data match

**Note**: Full testing requires ARM hardware platform support and U-Boot environment


## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details