# DMA API

[![Rust](https://github.com/drivercraft/dma-api/actions/workflows/rust.yml/badge.svg?branch=main)](https://github.com/drivercraft/dma-api/actions/workflows/rust.yml)

## Example

```rust
use dma_api::*;

// ----- Driver Side -----

// use global allocator to alloc `to device` type memory
let mut dma: DVec<u32> = DVec::zeros(10, 0x1000, Direction::ToDevice).unwrap();
// flush cache to memory.
dma.set(0, 1);

// do nothing with cache
let o = dma.get(0).unwrap();

assert_eq!(o, 1);


// ----- OS Side -----

struct Impled;

impl Impl for Impled {
    fn map(addr: std::ptr::NonNull<u8>, size: usize, direction: Direction) -> u64 {
        println!("map @{:?}, size {size:#x}, {direction:?}", addr);
        addr.as_ptr() as usize as _
    }

    fn unmap(addr: std::ptr::NonNull<u8>, size: usize) {
        println!("unmap @{:?}, size {size:#x}", addr);
    }

    fn flush(addr: std::ptr::NonNull<u8>, size: usize) {
        println!("flush @{:?}, size {size:#x}", addr);
    }

    fn invalidate(addr: std::ptr::NonNull<u8>, size: usize) {
        println!("invalidate @{:?}, size {size:#x}", addr);
    }
}

set_impl!(Impled);

// then you can do some thing with the driver.
```
