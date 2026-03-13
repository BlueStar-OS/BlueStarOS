# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release of the sdmmc crate
- Support for eMMC 4.x/5.x standards
- Support for SD 1.0/2.0 and SDIO devices
- PIO (Programmed I/O) data transfer mode
- DMA (Direct Memory Access) data transfer mode
- High-speed modes: HS, HS200, HS400
- Support for multiple bus widths (1-bit, 4-bit, 8-bit)
- Rockchip RK3568/RK3588 platform optimizations
- DWCMSHC controller support
- Type-safe register access
- no_std compatibility

### Platform Support
- Rockchip RK3568
- Rockchip RK3588
- Other platforms using DWCMSHC SDHCI controller

### Features
- `pio` (default): PIO mode for data transfer
- `dma`: DMA mode for data transfer (requires `dma-api`)

### Documentation
- Comprehensive API documentation
- Examples for basic usage
- Hardware-specific notes for supported platforms
