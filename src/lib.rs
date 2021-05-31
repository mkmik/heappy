//! The standard API includes: the [`malloc`], [`calloc`], [`realloc`], and
//! [`free`], which conform to to ISO/IEC 9899:1990 (“ISO C90”),
//! [`posix_memalign`] which conforms to conforms to POSIX.1-2016, and
//! [`aligned_alloc`].

use backtrace::Frame;

mod shim;

pub const MAX_DEPTH: usize = 32;

struct StaticBacktrace {
    frames: [Frame; MAX_DEPTH],
    size: usize,
}

impl StaticBacktrace {
    unsafe fn new() -> Self {
        Self {
            frames: std::mem::MaybeUninit::uninit().assume_init(),
            size: 0,
        }
    }

    unsafe fn push(&mut self, frame: &Frame) -> bool {
        self.frames[self.size] = frame.clone();
        self.size += 1;
        self.size < MAX_DEPTH
    }
}

impl<'a> IntoIterator for &'a StaticBacktrace {
    type Item = &'a Frame;
    type IntoIter = StaticBacktraceIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        StaticBacktraceIterator(self, 0)
    }
}

struct StaticBacktraceIterator<'a>(&'a StaticBacktrace, usize);

impl<'a> Iterator for StaticBacktraceIterator<'a> {
    type Item = &'a Frame;

    fn next(&mut self) -> Option<Self::Item> {
        if self.1 < self.0.size {
            let res = Some(&self.0.frames[self.1]);
            self.1 += 1;
            res
        } else {
            None
        }
    }
}

pub(crate) unsafe fn track_allocated(size: usize) {
    println!("allocated {}", size);

    let mut bt = StaticBacktrace::new();
    backtrace::trace(|frame| bt.push(frame));

    for frame in &bt {
        backtrace::resolve_frame(frame, |symbol| {
            if let Some(name) = symbol.name() {
                println!("{:#}", name);
            }
        });
    }

    /*
    let mut bt = backtrace::Backtrace::new_unresolved();
    bt.resolve();

    for frame in bt.frames() {
        for symbol in frame.symbols() {
            if let Some(name) = symbol.name() {
                println!("{:#}", name);
            }
        }
    }
     */

    /*
    backtrace::trace(|frame| {
        backtrace::resolve_frame(frame, |symbol| {
            if let Some(name) = symbol.name() {
                println!("{:#}", name);
            }
        });

        true // keep going to the next frame
    });

     */
}

pub fn demo() {
    println!("demo");
}
