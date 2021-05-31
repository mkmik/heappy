//! The standard API includes: the [`malloc`], [`calloc`], [`realloc`], and
//! [`free`], which conform to to ISO/IEC 9899:1990 (“ISO C90”),
//! [`posix_memalign`] which conforms to conforms to POSIX.1-2016, and
//! [`aligned_alloc`].

mod shim;

pub(crate) fn track_allocated(size: usize) {
    println!("allocated {}", size);
}

pub fn demo() {
    println!("demo");
}
