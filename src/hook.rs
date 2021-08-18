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
    let res = sys_malloc(size);
    Profiler::track_allocated(sys_malloc_usable_size(res) as isize);
    res
}

#[no_mangle]
pub unsafe extern "C" fn calloc(number: size_t, size: size_t) -> *mut c_void {
    let res = sys_calloc(number, size);
    Profiler::track_allocated(sys_malloc_usable_size(res) as isize);
    res
}

#[no_mangle]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    #[cfg(feature = "measure_free")]
    {
        let size = sys_malloc_usable_size(ptr) as isize;
        Profiler::track_allocated(-size);
    }
    sys_free(ptr)
}

#[no_mangle]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: size_t) -> *mut c_void {
    let old_size = sys_malloc_usable_size(ptr) as isize;
    let res = sys_realloc(ptr, size);
    Profiler::track_allocated(sys_malloc_usable_size(res) as isize - old_size);
    res
}

#[no_mangle]
pub unsafe extern "C" fn malloc_usable_size(ptr: *const c_void) -> size_t {
    sys_malloc_usable_size(ptr)
}

#[no_mangle]
pub unsafe extern "C" fn posix_memalign(
    ptr: *mut *mut c_void,
    alignment: size_t,
    size: size_t,
) -> c_int {
    sys_posix_memalign(ptr, alignment, size)
}

#[no_mangle]
pub unsafe extern "C" fn aligned_alloc(alignment: size_t, size: size_t) -> *mut c_void {
    let res = sys_aligned_alloc(alignment, size);
    Profiler::track_allocated(sys_malloc_usable_size(res) as isize);
    res
}
