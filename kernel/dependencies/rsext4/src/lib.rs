#![no_std]
#![deny(unused)]
#![deny(dead_code)]
#![deny(warnings)]
#![feature(iter_repeat_n,let_chains)]


extern crate alloc;
pub mod ext4_backend;
pub use ext4_backend::api::*;
pub use ext4_backend::blockdev::*;
pub use ext4_backend::config::*;
pub use ext4_backend::dir::*;
pub use ext4_backend::ext4::*;
pub use ext4_backend::file::*;
pub use ext4_backend::error::*;
