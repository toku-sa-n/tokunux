#![cfg_attr(not(test), no_std)]

use {
    arrayvec::ArrayVec,
    core::{convert::TryInto, fmt},
    os_units::NumOfPages,
    uefi_wrapper::service::boot::{MemoryDescriptor, CONVENTIONAL_MEMORY},
    x86_64::{
        structures::paging::{
            FrameAllocator as FrameAllocatorTrait, FrameDeallocator, PhysFrame, Size4KiB,
        },
        PhysAddr,
    },
};

const REASONABLE_MAX_DESCRIPTORS: usize = 128;

#[derive(PartialEq, Eq, Debug)]
pub struct FrameAllocator(ArrayVec<FrameDescriptor, REASONABLE_MAX_DESCRIPTORS>);
impl FrameAllocator {
    #[must_use]
    pub const fn new() -> Self {
        Self(ArrayVec::new_const())
    }

    pub fn init(&mut self, mmap: &[MemoryDescriptor]) {
        for descriptor in mmap {
            if is_conventional(descriptor) {
                self.init_for_descriptor(descriptor);
            }
        }
    }

    fn init_for_descriptor(&mut self, descriptor: &MemoryDescriptor) {
        let start = PhysAddr::new(descriptor.physical_start);
        let num = NumOfPages::new(descriptor.number_of_pages.try_into().unwrap());
        let frames = FrameDescriptor::new_for_available(start, num);

        self.0.push(frames);
    }
}
impl FrameAllocator {
    pub fn alloc(&mut self, num_of_pages: NumOfPages<Size4KiB>) -> Option<PhysAddr> {
        for i in 0..self.0.len() {
            if self.0[i].is_available_for_allocating(num_of_pages) {
                return Some(self.alloc_from_frames_at(i, num_of_pages));
            }
        }

        None
    }

    fn alloc_from_frames_at(&mut self, i: usize, n: NumOfPages<Size4KiB>) -> PhysAddr {
        if self.0[i].is_splittable(n) {
            self.split_frames(i, n);
        }

        self.0[i].available = false;
        self.0[i].start
    }

    fn split_frames(&mut self, i: usize, num_of_pages: NumOfPages<Size4KiB>) {
        assert!(self.0[i].available, "Frames are not available.");
        assert!(
            self.0[i].num_of_pages > num_of_pages,
            "Insufficient number of frames."
        );

        self.split_frames_unchecked(i, num_of_pages)
    }

    fn split_frames_unchecked(&mut self, i: usize, requested: NumOfPages<Size4KiB>) {
        let new_frames_start = self.0[i].start + requested.as_bytes().as_usize();
        let new_frames_num = self.0[i].num_of_pages - requested;
        let new_frames = FrameDescriptor::new_for_available(new_frames_start, new_frames_num);

        self.0[i].num_of_pages = requested;
        self.0.insert(i + 1, new_frames);
    }
}
impl FrameAllocator {
    pub fn dealloc(&mut self, addr: PhysAddr) {
        for i in 0..self.0.len() {
            if self.0[i].start == addr && !self.0[i].available {
                return self.free_memory_for_frames_at(i);
            }
        }
    }

    fn free_memory_for_frames_at(&mut self, i: usize) {
        self.0[i].available = true;
        self.merge_before_and_after_frames(i);
    }

    fn merge_before_and_after_frames(&mut self, i: usize) {
        if self.mergeable_to_next_frames(i) {
            self.merge_to_next_frames(i);
        }

        if i > 0 && self.mergeable_to_next_frames(i - 1) {
            self.merge_to_next_frames(i - 1);
        }
    }

    fn mergeable_to_next_frames(&self, i: usize) -> bool {
        if i >= self.0.len() - 1 {
            return false;
        }

        let node = &self.0[i];
        let next = &self.0[i + 1];

        node.is_mergeable(next)
    }

    fn merge_to_next_frames(&mut self, i: usize) {
        let n = self.0[i + 1].num_of_pages;
        self.0[i].num_of_pages += n;
        self.0.remove(i + 1);
    }
}
unsafe impl FrameAllocatorTrait<Size4KiB> for FrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let addr = self.alloc(NumOfPages::new(1))?;
        Some(PhysFrame::from_start_address(addr).unwrap())
    }
}
impl FrameDeallocator<Size4KiB> for FrameAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let addr = frame.start_address();
        self.dealloc(addr);
    }
}

#[derive(PartialEq, Eq)]
struct FrameDescriptor {
    start: PhysAddr,
    num_of_pages: NumOfPages<Size4KiB>,
    available: bool,
}
impl FrameDescriptor {
    fn new_for_available(start: PhysAddr, num_of_pages: NumOfPages<Size4KiB>) -> Self {
        Self {
            start,
            num_of_pages,
            available: true,
        }
    }

    #[cfg(test)]
    fn new_for_used(start: PhysAddr, num_of_pages: NumOfPages<Size4KiB>) -> Self {
        Self {
            start,
            num_of_pages,
            available: false,
        }
    }

    fn is_splittable(&self, requested: NumOfPages<Size4KiB>) -> bool {
        self.num_of_pages > requested
    }

    fn is_available_for_allocating(&self, request_num_of_pages: NumOfPages<Size4KiB>) -> bool {
        self.num_of_pages >= request_num_of_pages && self.available
    }

    fn is_mergeable(&self, other: &Self) -> bool {
        self.available && other.available && self.is_consecutive(other)
    }

    fn is_consecutive(&self, other: &Self) -> bool {
        self.end() == other.start
    }

    fn end(&self) -> PhysAddr {
        self.start + self.num_of_pages.as_bytes().as_usize()
    }
}
impl fmt::Debug for FrameDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let suffix = if self.available { "Available" } else { "Used" };
        write!(
            f,
            "Frames::<{}>({:?} .. {:?})",
            suffix,
            self.start,
            self.end()
        )
    }
}

fn is_conventional(d: &MemoryDescriptor) -> bool {
    d.r#type == CONVENTIONAL_MEMORY
}

#[cfg(test)]
mod tests {
    use super::{FrameAllocator, FrameDescriptor};
    use os_units::NumOfPages;
    use x86_64::PhysAddr;

    macro_rules! frames {
        (A $start:expr => $end:expr) => {
            FrameDescriptor::new_for_available(
                PhysAddr::new($start),
                os_units::Bytes::new($end - $start).as_num_of_pages(),
            )
        };
        (U $start:expr => $end:expr) => {
            FrameDescriptor::new_for_used(
                PhysAddr::new($start),
                os_units::Bytes::new($end - $start).as_num_of_pages(),
            )
        };
    }

    macro_rules! arrayvec {
        ($($elem:expr),*) => {
            {
                let mut v=arrayvec::ArrayVec::new();
                $(
                    v.push($elem);
                )*
                v
            }
        };
    }

    macro_rules! allocator {
        ($($is_available:ident $start:expr => $end:expr),*$(,)*) => {
            FrameAllocator(arrayvec![
                $(frames!($is_available $start => $end)),*
            ]
            )
        };
    }

    #[test]
    fn fail_to_allocate() {
        let mut f = allocator!(
            A 0 => 0x1000,
            A 0x2000 => 0xc000,
            U 0xc000 => 0x10000,
            U 0x10000 => 0x13000,
            A 0x13000 => 0x15000,
        );

        let a = f.alloc(NumOfPages::new(200));
        assert!(a.is_none());
    }

    #[test]
    fn allocate_not_power_of_two() {
        let mut f = allocator!(
            A 0 => 0x1000,
            A 0x2000 => 0xc000,
            U 0xc000 => 0x10000,
        );

        let a = f.alloc(NumOfPages::new(3));

        assert_eq!(a, Some(PhysAddr::new(0x2000)));
        assert_eq!(
            f,
            allocator!(
                A 0 => 0x1000,
                U 0x2000 => 0x5000,
                A 0x5000 => 0xc000,
                U 0xc000 => 0x10000,
            )
        )
    }

    #[test]
    fn allocate_full_frames() {
        let mut f = allocator!(A 0 => 0x3000);
        let a = f.alloc(NumOfPages::new(3));

        assert_eq!(a, Some(PhysAddr::zero()));
        assert_eq!(f, allocator!(U 0 => 0x3000));
    }

    #[test]
    fn free_single_frames() {
        let mut f = allocator!(U 0 => 0x3000);
        f.dealloc(PhysAddr::zero());

        assert_eq!(f, allocator!(A 0 => 0x3000));
    }

    #[test]
    fn free_and_merge_with_before() {
        let mut f = allocator!(
            A 0 => 0x1000,
            A 0x2000 => 0xc000,
            U 0xc000 => 0x10000,
        );

        f.dealloc(PhysAddr::new(0xc000));

        assert_eq!(
            f,
            allocator! (
                A 0 => 0x1000,
                A 0x2000 => 0x10000
            )
        )
    }

    #[test]
    fn free_and_merge_with_after() {
        let mut f = allocator!(
            U 0 => 0x3000,
            A 0x3000 => 0x5000,
        );

        f.dealloc(PhysAddr::zero());

        assert_eq!(
            f,
            allocator!(
                A 0 => 0x5000,
            )
        )
    }

    #[test]
    fn free_and_merge_with_before_and_after() {
        let mut f = allocator!(
            A 0 => 0x3000,
            U 0x3000 => 0x5000,
            A 0x5000 => 0x10000,
        );

        f.dealloc(PhysAddr::new(0x3000));

        assert_eq!(f, allocator!(A 0 => 0x10000))
    }

    #[test]
    fn mergable_two_frmaes() {
        let f1 = frames!(A 0x2000 => 0xc000);
        let f2 = frames!(A 0xc000 => 0x10000);

        assert!(f1.is_mergeable(&f2));
    }
}
