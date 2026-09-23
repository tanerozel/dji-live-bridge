//! Custom allocator hooks for librtmp2
//!
//! Mirrors `src/core/alloc.h` and `src/core/alloc.c`.
//! Provides pluggable allocation functions with standard defaults.

use parking_lot::RwLock;
use std::sync::atomic::{AtomicPtr, Ordering};

/// Allocator function type.
pub type AllocFn = fn(size: usize, userdata: *mut u8) -> *mut u8;
/// Reallocator function type.
pub type ReallocFn = fn(ptr: *mut u8, size: usize, userdata: *mut u8) -> *mut u8;
/// Free function type.
pub type FreeFn = fn(ptr: *mut u8, userdata: *mut u8);

struct AllocatorHooks {
    alloc: AllocFn,
    realloc: ReallocFn,
    free: FreeFn,
    userdata: AtomicPtr<u8>,
}

impl AllocatorHooks {
    const fn std_defaults() -> Self {
        Self {
            alloc: std_alloc,
            realloc: std_realloc,
            free: std_free,
            userdata: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    fn userdata_ptr(&self) -> *mut u8 {
        self.userdata.load(Ordering::Acquire)
    }
}

static ALLOCATOR: RwLock<AllocatorHooks> = RwLock::new(AllocatorHooks::std_defaults());

fn std_alloc(size: usize, _ud: *mut u8) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    unsafe { libc::malloc(size) as *mut u8 }
}

fn std_realloc(ptr: *mut u8, size: usize, _ud: *mut u8) -> *mut u8 {
    if ptr.is_null() {
        return std_alloc(size, _ud);
    }
    if size == 0 {
        std_free(ptr, _ud);
        return std::ptr::null_mut();
    }
    unsafe { libc::realloc(ptr as *mut libc::c_void, size) as *mut u8 }
}

fn std_free(ptr: *mut u8, _ud: *mut u8) {
    if !ptr.is_null() {
        unsafe { libc::free(ptr as *mut libc::c_void) }
    }
}

/// Set custom allocator functions.
pub fn set_allocator(alloc: AllocFn, realloc: ReallocFn, free: FreeFn, userdata: *mut u8) {
    let mut hooks = ALLOCATOR.write();
    hooks.alloc = alloc;
    hooks.realloc = realloc;
    hooks.free = free;
    hooks.userdata.store(userdata, Ordering::Release);
}

fn with_hooks<R>(f: impl FnOnce(&AllocatorHooks) -> R) -> R {
    let hooks = ALLOCATOR.read();
    f(&hooks)
}

/// Allocate `size` bytes using the current allocator.
pub fn lrtmp2_malloc(size: usize) -> *mut u8 {
    with_hooks(|hooks| (hooks.alloc)(size, hooks.userdata_ptr()))
}

/// Allocate zeroed memory for `nmemb` elements of `size` bytes each.
pub fn lrtmp2_calloc(nmemb: usize, size: usize) -> *mut u8 {
    if nmemb != 0 && size > usize::MAX / nmemb {
        return std::ptr::null_mut();
    }
    let total = nmemb * size;
    let p = lrtmp2_malloc(total);
    if !p.is_null() {
        unsafe {
            std::ptr::write_bytes(p, 0, total);
        }
    }
    p
}

/// Reallocate memory.
pub fn lrtmp2_realloc(ptr: *mut u8, size: usize) -> *mut u8 {
    with_hooks(|hooks| (hooks.realloc)(ptr, size, hooks.userdata_ptr()))
}

/// Free memory.
pub fn lrtmp2_free(ptr: *mut u8) {
    if !ptr.is_null() {
        with_hooks(|hooks| (hooks.free)(ptr, hooks.userdata_ptr()));
    }
}

/// Allocate a Vec-backed buffer (idiomatic Rust helper).
pub fn alloc_vec<T: Copy + Default>(n: usize) -> Vec<T> {
    vec![T::default(); n]
}
