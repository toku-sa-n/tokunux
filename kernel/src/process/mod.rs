use {
    crate::sysproc,
    aligned_ptr::slice,
    context::Context,
    core::{cell::UnsafeCell, convert::TryInto},
    os_units::NumOfPages,
    pid::Pid,
    vm::{accessor::single::write_only, Kbox},
    x86_64::{
        registers::control::Cr3,
        structures::paging::{FrameAllocator, PageTableFlags, PhysFrame, Size4KiB},
        VirtAddr,
    },
};

pub(crate) use manager::switch;

mod context;
mod manager;
mod pid;

const MAX_PROCESS: usize = 8;
const GUARD_PAGE_SIZE: usize = 4096;
const KERNEL_STACK_BYTES: usize = 12288;

pub(super) fn init() {
    manager::add_idle();

    manager::add(Process::from_initrd("init"));
    manager::add(Process::from_function(sysproc::main));
}

#[derive(Debug)]
pub(super) struct Process {
    pid: Pid,
    context: UnsafeCell<Context>,

    kernel_stack: Kbox<UnsafeCell<[u8; KERNEL_STACK_BYTES]>>,
}
impl Process {
    const KERNEL_STACK_MAGIC: [u8; 8] = [0x73, 0x74, 0x6b, 0x67, 0x75, 0x61, 0x72, 0x64];

    fn idle() -> Self {
        Self {
            pid: Pid::new(0),
            context: UnsafeCell::default(),
            kernel_stack: Self::generate_kernel_stack(),
        }
    }

    fn from_function(f: fn() -> !) -> Self {
        Self::try_from_function(f).expect("Failed to create a process from a function.")
    }

    fn try_from_function(f: fn() -> !) -> Option<Self> {
        let pid = Pid::generate()?;

        let pml4 = Self::create_new_pml4()?;

        let entry = VirtAddr::new((f as usize).try_into().unwrap());

        let mut kernel_stack = Self::generate_kernel_stack();

        let kernel_stack_len = kernel_stack.get_mut().len();

        let kernel_stack_end = VirtAddr::from_ptr(kernel_stack.get()) + kernel_stack_len;

        unsafe {
            Self::switch_pml4_do(pml4, || {
                let context = Context::kernel(entry, pml4, kernel_stack_end);
                let context = UnsafeCell::new(context);

                Some(Self {
                    pid,
                    context,
                    kernel_stack,
                })
            })
        }
    }

    fn from_initrd(name: &str) -> Self {
        Self::try_from_initrd(name)
            .unwrap_or_else(|| panic!("Failed to create the {} process.", name))
    }

    fn try_from_initrd(name: &str) -> Option<Self> {
        let pid = Pid::generate()?;

        let stack_size = NumOfPages::new(5);

        let pml4 = Self::create_new_pml4()?;

        let file = cpio_reader::iter_files(initrd()).find(|f| f.name() == name)?;

        let binary = file.file();

        let stack_flags = PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::USER_ACCESSIBLE
            | PageTableFlags::NO_EXECUTE;

        // SAFETY: `pml4` is generated in this method.
        unsafe {
            Self::switch_pml4_do(pml4, || {
                let entry = vm::map_elf(binary);

                let stack_range = vm::alloc_pages(stack_size, stack_flags).unwrap();

                let context = Context::user(entry, pml4, stack_range.end.start_address());
                let context = UnsafeCell::new(context);

                Some(Self {
                    pid,
                    context,
                    kernel_stack: Self::generate_kernel_stack(),
                })
            })
        }
    }

    fn check_kernel_stack_guard(&self) {
        // SAFETY: The borrow checker ensures that there is no mutable references to the kernel
        // stack.
        let stack = unsafe { &*self.kernel_stack.get() };

        let magic = &stack[GUARD_PAGE_SIZE..GUARD_PAGE_SIZE + Self::KERNEL_STACK_MAGIC.len()];

        if magic != Self::KERNEL_STACK_MAGIC {
            panic!("The kernel stack is smashed.");
        }
    }

    /// # Safety
    ///
    /// `pml4` must be a correct PML4.
    unsafe fn switch_pml4_do<T>(pml4: PhysFrame, f: impl FnOnce() -> T) -> T {
        let (old_pml4, flags) = Cr3::read();

        // SAFETY: The caller must ensure that `pml4` is a correct PML4.
        unsafe {
            Cr3::write(pml4, flags);
        }

        let r = f();

        // SAFETY: `old_pml4` is surely a correct PML4.
        unsafe {
            Cr3::write(old_pml4, flags);
        }

        r
    }

    fn create_new_pml4() -> Option<PhysFrame> {
        let mut allocator = vm::frame_allocator();

        allocator.allocate_frame().map(|frame| {
            // To avoid a deadlock caused by `write_only` in `init_pml4`, which may allocate frames
            // when mapping pages.
            drop(allocator);

            // SAFETY: `frame` is allocated.
            unsafe {
                Self::init_pml4(frame);
            }

            frame
        })
    }

    /// # Safety
    ///
    /// `frame` must be allocated.
    unsafe fn init_pml4(frame: PhysFrame) {
        let mut pml4 = vm::current_pml4();

        for i in 0..510 {
            pml4[i].set_unused();
        }

        let flags = pml4[510].flags();

        pml4[510].set_frame(frame, flags);

        // SAFETY: The caller must ensure that `frame` is allocated.
        unsafe {
            write_only(frame.start_address()).write_volatile(pml4);
        }
    }

    fn kernel_stack_bottom_addr(&self) -> VirtAddr {
        let ptr = self.kernel_stack.get();

        // SAFETY: No references point to `kernel_stack`.
        VirtAddr::from_ptr(ptr) + unsafe { (&*ptr).len() }
    }

    fn generate_kernel_stack() -> Kbox<UnsafeCell<[u8; KERNEL_STACK_BYTES]>> {
        let mut stack = Kbox::new(UnsafeCell::new([0; KERNEL_STACK_BYTES]));

        for (i, c) in Self::KERNEL_STACK_MAGIC.iter().enumerate() {
            stack.get_mut()[GUARD_PAGE_SIZE + i] = *c;
        }

        stack
    }
}

fn initrd<'a>() -> &'a [u8] {
    use predefined_mmap::initrd;

    let num_of_pages = initrd().end - initrd().start;
    let num_of_pages = NumOfPages::<Size4KiB>::new(num_of_pages.try_into().unwrap());

    let start = initrd().start.start_address().as_ptr();

    // SAFETY: No mutable references point to this region.
    unsafe { slice::from_raw_parts(start, num_of_pages.as_bytes().as_usize()) }
}
