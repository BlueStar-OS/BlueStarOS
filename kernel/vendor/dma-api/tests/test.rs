use dma_api::*;

#[test]
fn test_read() {
    let mut dma: DVec<u32> = DVec::zeros(10, 0x1000, Direction::FromDevice).unwrap();

    dma.set(0, 1);

    let o = dma.get(0).unwrap();

    assert_eq!(o, 1);
}

#[test]
fn test_write() {
    let mut dma: DVec<u32> = DVec::zeros(10, 0x1000, Direction::ToDevice).unwrap();

    dma.set(0, 1);

    let o = dma.get(0).unwrap();

    assert_eq!(o, 1);
}
#[derive(Debug, PartialEq, Eq)]
struct Foo {
    foo: u32,
    bar: u32,
}

#[test]
fn test_modify() {
    let mut dma: DBox<Foo> = DBox::zero(Direction::Bidirectional).unwrap();

    dma.modify(|f| f.bar = 1);

    assert_eq!(dma.read(), Foo { foo: 0, bar: 1 });
}

#[test]
fn test_deref() {
    let mut dma: DVec<u32> = DVec::zeros(10, 0x1000, Direction::FromDevice).unwrap();

    dma.set(0, 1);

    let foo = &dma;

    assert_eq!(foo[0], 1);
}

#[test]
fn test_copy() {
    let mut dma: DVec<u32> = DVec::zeros(0x40, 0x1000, Direction::Bidirectional).unwrap();

    println!("new dma ok");

    let src = [1u32; 0x40];

    dma.copy_from_slice(&src);

    println!("copy ok");

    assert!(dma.eq(&src));
}

#[test]
fn test_index() {
    let dma: DVec<u32> = DVec::zeros(0x40, 0x1000, Direction::Bidirectional).unwrap();

    println!("new dma ok");

    let a = dma[0];

    assert_eq!(a, 0);
}

#[test]
fn test_slice() {
    let src = [1u32; 0x40];
    let dma = DSlice::from(src.as_ref());

    assert!(dma.eq(&src));
}

#[test]
fn test_slice_index() {
    let src = [1u32; 0x40];
    let dma = DSlice::from(src.as_ref());

    assert_eq!(dma[1], 1);
}

#[test]
fn test_slice_mut() {
    let mut src = [1u32; 0x40];
    let dma = DSliceMut::from(src.as_mut(), Direction::Bidirectional);

    dma.set(0, 2);

    assert_eq!(dma[0], 2);
}

#[test]
fn test_from_vec() {
    let value = vec![1, 2, 3];
    let dma = DVec::from_vec(value, Direction::FromDevice);

    assert_eq!(dma[1], 2);

    let v = dma.to_vec();

    println!("to vec");

    assert_eq!(v, vec![1, 2, 3]);
}

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
