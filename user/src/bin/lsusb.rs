#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::{print, println, sys_close, sys_open, sys_read, O_RDONLY};

const USB_DEVICE_LIST: &str = "/sys/bus/usb/devices/list";

fn decimal(field: &[u8]) -> Option<u8> {
    if field.len() != 3 || !field.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some((field[0] - b'0') * 100 + (field[1] - b'0') * 10 + field[2] - b'0')
}

fn hexadecimal(field: &[u8]) -> Option<u16> {
    if field.len() != 4 {
        return None;
    }
    let mut value = 0u16;
    for byte in field {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return None,
        };
        value = (value << 4) | u16::from(digit);
    }
    Some(value)
}

fn print_record(line: &[u8]) {
    // `/sys/bus/usb/devices/list`: "BBB DDD VVVV PPPP configs".
    if line.len() < 17 || line[3] != b' ' || line[7] != b' ' || line[12] != b' ' {
        return;
    }
    let Some(bus) = decimal(&line[0..3]) else {
        return;
    };
    let Some(device) = decimal(&line[4..7]) else {
        return;
    };
    let Some(vendor) = hexadecimal(&line[8..12]) else {
        return;
    };
    let Some(product) = hexadecimal(&line[13..17]) else {
        return;
    };
    // Product strings need USB string-descriptor support; until then retain
    // the familiar lsusb prefix and present just the stable numeric identity.
    println!(
        "Bus {:03} Device {:03}: ID {:04x}:{:04x}",
        bus, device, vendor, product
    );
}

#[no_mangle]
pub fn main() -> usize {
    let fd = sys_open(USB_DEVICE_LIST, O_RDONLY);
    if fd < 0 {
        println!("lsusb: cannot open {}", USB_DEVICE_LIST);
        return 1;
    }

    // QEMU's current xHCI exposes at most 64 slots; 2 KiB covers its fixed
    // one-line-per-device sysfs view in one read.
    let mut buffer = [0u8; 2048];
    let read = sys_read(fd as usize, buffer.as_mut_ptr() as usize, buffer.len());
    if read < 0 {
        let _ = sys_close(fd as usize);
        println!("lsusb: cannot read {}", USB_DEVICE_LIST);
        return 1;
    }

    let mut start = 0usize;
    for offset in 0..read as usize {
        if buffer[offset] == b'\n' {
            print_record(&buffer[start..offset]);
            start = offset + 1;
        }
    }
    // Consume EOF so the shared minimal sysfs file resets its read cursor for
    // the next `lsusb` invocation.
    let _ = sys_read(fd as usize, buffer.as_mut_ptr() as usize, buffer.len());
    let _ = sys_close(fd as usize);
    0
}
