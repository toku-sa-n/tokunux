#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

mod allocator;
pub mod elf;
mod exit_boot_services;
pub mod fs;
pub mod gop;
pub mod io;
mod mapper;
mod paging;
pub mod panic;
pub mod system_table;

pub(crate) use allocator::Allocator;
pub use exit_boot_services::exit_boot_services_and_return_mmap;
pub use system_table::SystemTable;
