#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate rlibc as _;

pub mod gdt;
pub mod idt;
pub mod mem;

#[cfg(test_on_qemu)]
use x86_64::structures::paging::Size4KiB;
use {
    aligned_ptr::ptr, boot_info::BootInfo, core::panic::PanicInfo, qemu_print::qemu_println,
};

#[cfg(test_on_qemu)]
pub(crate) type NumOfPages<T = Size4KiB> = os_units::NumOfPages<T>;

pub fn init(boot_info: &mut BootInfo) {
    // SAFETY: `boot_info` is the pointer passed from the bootloader. w
    let mut boot_info = unsafe { ptr::get(boot_info) };
    boot_info.validate();

    // SAFETY: The recursive address is accessible and there are no references to the current
    // working PML4.
    unsafe { mem::init(boot_info.mmap().as_slice()) };

    gdt::init();
    idt::init();
}

#[cfg(test_on_qemu)]
pub fn fini() -> ! {
    qemu::exit_success();
}

#[cfg(not(test_on_qemu))]
pub fn fini() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(i: &PanicInfo<'_>) -> ! {
    qemu_println!("{}", i);

    exit_panic();
}

#[cfg(test_on_qemu)]
pub fn exit_panic() -> ! {
    qemu::exit_failure();
}

#[cfg(not(test_on_qemu))]
pub fn exit_panic() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}
