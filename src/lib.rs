mod profiler;
pub use profiler::*;

mod collector;
#[cfg(feature = "enable_heap_profiler")]
mod hook;

#[cfg(feature = "jemalloc_shim")]
mod jemalloc_adapter;

#[cfg(feature = "jemalloc_shim")]
use jemalloc_adapter as adapter;

// On linux you need to reference at least one symbol in a module if we want it be be actually linked.
// Otherwise the hooks like `pub unsafe extern "C" fn malloc(size: size_t) -> *mut c_void` defined in the shim
// module won't override the respective weak symbols from libc, since they don't ever get linked in the final executable.
// On macos this is not necessary, but it doesn't hurt.
// (e.g. the functions that override weak symbols exported by libc)
#[cfg(feature = "enable_heap_profiler")]
pub fn dummy_force_link() {
    hook::dummy_force_link();
}
