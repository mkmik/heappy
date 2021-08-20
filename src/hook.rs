//! The standard API includes: the [`malloc`], [`calloc`], [`realloc`], and
//! [`free`], which conform to to ISO/IEC 9899:1990 (“ISO C90”),
//! [`posix_memalign`] which conforms to conforms to POSIX.1-2016, and
//! [`aligned_alloc`].

use crate::adapter::*;
use crate::profiler::Profiler;
use libc::{c_int, c_void, size_t};

// On linux we need to reference at least one symbol in a module for it to not be pruned at link time.
pub(crate) fn dummy_force_link() {}

#[no_mangle]
pub unsafe extern "C" fn malloc(size: size_t) -> *mut c_void {
    let ptr = sys_malloc(size + Block::header_size());
    let mut block = Block::new(ptr);
    block.set_size(size);
    Profiler::track_allocated(size as isize, &mut block);
    block.user_payload()
}

#[no_mangle]
pub unsafe extern "C" fn calloc(number: size_t, size: size_t) -> *mut c_void {
    let mut extra = Block::header_size() / size;
    let block_size = (number + extra) * size;
    if block_size < (number * size + Block::header_size()) {
        extra += 1;
    };
    let ptr = sys_calloc(number + extra, size);
    let mut block = Block::new(ptr);
    let effective_size = number * size;
    block.set_size(effective_size);
    Profiler::track_allocated(effective_size as isize, &mut block);
    block.user_payload()
}

#[no_mangle]
pub unsafe extern "C" fn free(body: *mut c_void) {
    // if free is called with a NULL parameter, no operation is performed.
    if body.is_null() {
        return;
    }

    let mut block = Block::adopt(body);
    if !block.check() {
        sys_free(body);
        return;
    }

    #[cfg(feature = "measure_free")]
    {
        let size = block.size() as isize;
        Profiler::track_allocated(-size, &mut block);
    }
    block.free();
    sys_free(block.ptr())
}

#[no_mangle]
pub unsafe extern "C" fn realloc(body: *mut c_void, size: size_t) -> *mut c_void {
    // if realloc is called with a NULL argument, it behaves like malloc
    if body.is_null() {
        return malloc(size);
    }

    let mut block = Block::adopt(body);
    if !block.check() {
        return sys_realloc(body, size);
    }

    let old_size = block.set_size(size);
    let base = sys_realloc(block.ptr(), size + Block::header_size());
    Profiler::track_allocated(size as isize - old_size as isize, &mut block);
    block.rebase(base);
    block.user_payload()
}

#[no_mangle]
pub unsafe extern "C" fn malloc_usable_size(ptr: *const c_void) -> size_t {
    sys_malloc_usable_size(ptr)
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(
    ptrptr: *mut *mut c_void,
    alignment: size_t,
    size: size_t,
) -> c_int {
    let res = sys_posix_memalign(ptrptr, alignment, size + Block::header_size());
    if res != 0 {
        return res;
    }
    let ptr = ptrptr.read();
    let mut block = Block::new(ptr);
    block.set_size(size);
    Profiler::track_allocated(size as isize, &mut block);

    ptrptr.write(block.user_payload());

    res
}

#[no_mangle]
pub unsafe extern "C" fn aligned_alloc(alignment: size_t, size: size_t) -> *mut c_void {
    let ptr = sys_aligned_alloc(alignment, size + Block::header_size());
    let mut block = Block::new(ptr);
    block.set_size(size);
    Profiler::track_allocated(size as isize, &mut block);
    block.user_payload()
}
