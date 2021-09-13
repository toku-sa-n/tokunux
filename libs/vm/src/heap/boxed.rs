use core::{
    alloc::Layout,
    fmt,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

pub struct Kbox<T> {
    ptr: NonNull<T>,
    _marker: PhantomData<T>,
}
impl<T> Kbox<T> {
    pub fn new(x: T) -> Self {
        let p: *mut T = super::alloc(Layout::new::<T>()).cast();
        let ptr = NonNull::new(p).expect("Failed to allocate memory.");

        // SAFETY: The pointer points to the allocated memory.
        unsafe {
            ptr.as_ptr().write(x);
        }

        Self {
            ptr,
            _marker: PhantomData,
        }
    }
}
impl<T> Clone for Kbox<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.deref().clone())
    }
}
impl<T> Deref for Kbox<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: The pointer points to the allocated, and initialized value.
        unsafe { self.ptr.as_ref() }
    }
}
impl<T> DerefMut for Kbox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: The pointer points to the allocated, and initialized value.
        unsafe { self.ptr.as_mut() }
    }
}
impl<T: fmt::Debug> fmt::Debug for Kbox<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&**self, f)
    }
}
impl<T> Drop for Kbox<T> {
    fn drop(&mut self) {
        // SAFETY: The pointer is generated by `alloc`, and the layout is the same as the one used
        // to allocate the memory.
        unsafe {
            super::dealloc(self.ptr.as_ptr().cast(), Layout::new::<T>());
        }
    }
}
unsafe impl<T: Send> Send for Kbox<T> {}
unsafe impl<T: Sync> Sync for Kbox<T> {}
