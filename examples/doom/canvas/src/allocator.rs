//! C and Rust must share one allocator, not independently allocate from __heap_base.
//! Function pointers cross a C shim so malloc/free do not become forbidden wasm exports.
//! GPL-2.0; see ../../engine/LICENSE.TXT.
use std::alloc::{alloc, alloc_zeroed, dealloc, realloc as rust_realloc, Layout};
use std::ptr;

const HEADER: usize = 16;

unsafe extern "C" {
    fn doom_allocator_init(
        malloc: unsafe extern "C" fn(usize) -> *mut u8,
        calloc: unsafe extern "C" fn(usize, usize) -> *mut u8,
        free: unsafe extern "C" fn(*mut u8),
        realloc: unsafe extern "C" fn(*mut u8, usize) -> *mut u8,
    );
}

pub fn init() {
    // SAFETY: callbacks use the C ABI and remain valid for the module's lifetime.
    unsafe { doom_allocator_init(malloc, calloc, free, realloc) };
}

fn layout(size: usize) -> Option<Layout> {
    Layout::from_size_align(size.max(1).checked_add(HEADER)?, HEADER).ok()
}

unsafe fn allocate(size: usize, zero: bool) -> *mut u8 {
    let Some(layout) = layout(size) else {
        return ptr::null_mut();
    };
    let base = if zero {
        alloc_zeroed(layout)
    } else {
        alloc(layout)
    };
    if base.is_null() {
        return base;
    }
    base.cast::<usize>().write(size);
    base.add(HEADER)
}

unsafe extern "C" fn malloc(size: usize) -> *mut u8 {
    allocate(size, false)
}

unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut u8 {
    count
        .checked_mul(size)
        .map_or(ptr::null_mut(), |size| allocate(size, true))
}

unsafe extern "C" fn free(pointer: *mut u8) {
    if pointer.is_null() {
        return;
    }
    let base = pointer.sub(HEADER);
    let size = base.cast::<usize>().read();
    dealloc(base, layout(size).unwrap());
}

unsafe extern "C" fn realloc(pointer: *mut u8, size: usize) -> *mut u8 {
    if pointer.is_null() {
        return malloc(size);
    }
    if size == 0 {
        free(pointer);
        return ptr::null_mut();
    }
    let Some(new_layout) = layout(size) else {
        return ptr::null_mut();
    };
    let base = pointer.sub(HEADER);
    let old_size = base.cast::<usize>().read();
    let new_base = rust_realloc(base, layout(old_size).unwrap(), new_layout.size());
    if new_base.is_null() {
        return new_base;
    }
    new_base.cast::<usize>().write(size);
    new_base.add(HEADER)
}
