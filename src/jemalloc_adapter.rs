use libc::{c_int, c_void, size_t};

#[link(name = "jemalloc")]
extern "C" {
    #[link_name = "_rjem_malloc"]
    pub fn sys_malloc(size: size_t) -> *mut c_void;

    #[link_name = "_rjem_calloc"]
    pub fn sys_calloc(number: size_t, size: size_t) -> *mut c_void;

    #[link_name = "_rjem_free"]
    pub fn sys_free(ptr: *mut c_void);

    #[link_name = "_rjem_realloc"]
    pub fn sys_realloc(ptr: *mut c_void, size: size_t) -> *mut c_void;

    #[link_name = "_rjem_malloc_usable_size"]
    pub fn sys_malloc_usable_size(ptr: *const c_void) -> size_t;

    #[link_name = "_rjem_posix_memalign"]
    pub fn sys_posix_memalign(ptr: *mut *mut c_void, alignment: size_t, size: size_t) -> c_int;

    #[cfg(target_os = "macos")]
    #[link_name = "_rjem_posix_aligned_alloc"]
    pub fn sys_aligned_alloc(alignment: size_t, size: size_t) -> *mut c_void;

    #[cfg(not(target_os = "macos"))]
    #[link_name = "_rjem_aligned_alloc"]
    pub fn sys_aligned_alloc(alignment: size_t, size: size_t) -> *mut c_void;
}
