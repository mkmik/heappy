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

    fn iter<'a>(&'a self) -> StaticBacktraceIterator<'a> {
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

impl From<StaticBacktrace> for pprof::Frames {
    fn from(bt: StaticBacktrace) -> Self {
        let frames = bt
            .iter()
            .map(|frame| {
                let mut symbols: Vec<pprof::Symbol> = Vec::new();
                backtrace::resolve_frame(frame, |symbol| {
                    if let Some(name) = symbol.name() {
                        let name = format!("{:#}", name);
                        symbols.push(pprof::Symbol {
                            name: Some(name.as_bytes().to_vec()),
                            addr: None,
                            lineno: None,
                            filename: None,
                        })
                    }
                });
                symbols
            })
            .collect();
        Self {
            frames,
            thread_name: "".to_string(),
            thread_id: 0,
        }
    }
}

pub(crate) unsafe fn track_allocated(size: usize) {
    println!("allocated {}", size);

    let mut bt = StaticBacktrace::new();
    backtrace::trace(|frame| bt.push(frame));

    for frame in bt.iter() {
        backtrace::resolve_frame(frame, |symbol| {
            if let Some(name) = symbol.name() {
                println!("{:#}", name);
            }
        });
    }
}

pub fn demo() {
    println!("demo");
}
