#![no_std]
#![no_main]

// For `memcpy`.
extern crate rlibc as _;

use bootx64::fs;
use bootx64::gop;
use log::info;

#[no_mangle]
pub extern "win64" fn efi_main(h: bootx64::Handle, st: bootx64::SystemTable) -> ! {
    bootx64::init(h, st);

    info!("GOP info: {:?}", gop::set_preferred_resolution());
    let bytes = fs::locate("kernel");
    info!("{:X?}", &bytes[0..8]);

    loop {
        x86_64::instructions::hlt();
    }
}
