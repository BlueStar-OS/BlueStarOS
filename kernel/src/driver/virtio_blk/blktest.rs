use log::error;

use crate::driver::VirtBlk;




pub fn blktest(){
    let blk = VirtBlk::new();
    let mut buffe:[u8;512] = [0;512];
    let mut readbuffe:[u8;512] = [0;512];
    buffe[1]=b'f';
    blk.0.lock().write_block(0, &buffe);
    blk.0.lock().read_block(0, &mut readbuffe);
    error!("Iread:{:?}",readbuffe);
}