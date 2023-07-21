use super::page_size;
use std::alloc;

/// StackSize specifies desired stack size for new task.
///
/// It defaults to `8` times page size or [libc::MINSIGSTKSZ] depending on which one is bigger.
#[derive(Copy, Clone, Default, Debug)]
pub struct StackSize {
    size: isize,
}

impl StackSize {
    fn align_to_page_size(size: usize) -> usize {
        let mask = page_size::get() - 1;
        (size + mask) & !mask
    }

    fn aligned_page_size(&self) -> usize {
        let size = match self.size {
            0 => 8 * page_size::get(),
            1.. => 8 * page_size::get() + Self::align_to_page_size(self.size as usize),
            _ => Self::align_to_page_size((-self.size) as usize),
        };
        size.max(libc::MINSIGSTKSZ)
    }

    /// Specifies extra stack size in addition to default.
    pub fn with_extra_size(size: usize) -> StackSize {
        assert!(size <= isize::MAX as usize);
        StackSize {
            size: size as isize,
        }
    }

    /// Specifies desired stack size.
    pub fn with_size(size: usize) -> StackSize {
        assert!(size <= isize::MAX as usize, "stack size is too large");
        StackSize {
            size: -(size.max(1) as isize),
        }
    }
}

pub struct Stack {
    base: *mut u8,
    size: libc::size_t,
}

impl Stack {
    pub fn base(&self) -> *mut u8 {
        self.base
    }

    pub fn size(&self) -> usize {
        self.size as usize
    }

    pub fn alloc(size: StackSize) -> Stack {
        let page_size = page_size::get();
        let stack_size = size.aligned_page_size();
        let alloc_size = stack_size + 2 * page_size;
        let layout = unsafe { alloc::Layout::from_size_align_unchecked(alloc_size, page_size) };
        let stack_low = unsafe { alloc::alloc(layout) };
        let stack_base = unsafe { stack_low.add(page_size) };
        let stack_high = unsafe { stack_base.add(stack_size) };
        unsafe { libc::mprotect(stack_low as *mut libc::c_void, page_size, libc::PROT_NONE) };
        unsafe { libc::mprotect(stack_high as *mut libc::c_void, page_size, libc::PROT_NONE) };
        Stack {
            base: stack_base,
            size: stack_size,
        }
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        if self.base.is_null() {
            return;
        }
        let page_size = page_size::get();
        let alloc_size = self.size + 2 * page_size;
        let low = unsafe { self.base.sub(page_size) };
        let high = unsafe { self.base.add(self.size) };
        let prot = libc::PROT_READ | libc::PROT_WRITE;
        unsafe { libc::mprotect(low as *mut libc::c_void, page_size, prot) };
        unsafe { libc::mprotect(high as *mut libc::c_void, page_size, prot) };
        let layout = unsafe { alloc::Layout::from_size_align_unchecked(alloc_size, page_size) };
        unsafe { alloc::dealloc(low, layout) };
    }
}
