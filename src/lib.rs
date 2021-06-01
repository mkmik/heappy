//! The standard API includes: the [`malloc`], [`calloc`], [`realloc`], and
//! [`free`], which conform to to ISO/IEC 9899:1990 (“ISO C90”),
//! [`posix_memalign`] which conforms to conforms to POSIX.1-2016, and
//! [`aligned_alloc`].

use backtrace::Frame;
use pprof::protos::Message;
use spin::RwLock;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Write;

mod shim;

pub const MAX_DEPTH: usize = 32;

lazy_static::lazy_static! {
    pub(crate) static ref HEAP_PROFILER: RwLock<Profiler> = RwLock::new(Profiler::new());
}

struct Profiler {
    collector: pprof::Collector<StaticBacktrace>,
}

impl Profiler {
    fn new() -> Self {
        Self {
            collector: pprof::Collector::new().unwrap(),
        }
    }
}

struct StaticBacktrace {
    frames: [Frame; MAX_DEPTH],
    size: usize,
}

impl Clone for StaticBacktrace {
    fn clone(&self) -> Self {
        let mut n = unsafe { Self::new() };
        for i in 0..self.size {
            n.frames[i] = self.frames[i].clone()
        }
        n.size = self.size;
        n
    }
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

impl Hash for StaticBacktrace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iter()
            .for_each(|frame| frame.symbol_address().hash(state));
    }
}

impl PartialEq for StaticBacktrace {
    fn eq(&self, other: &Self) -> bool {
        Iterator::zip(self.iter(), other.iter())
            .map(|(s1, s2)| s1.symbol_address() == s2.symbol_address())
            .all(|equal| equal)
    }
}

impl Eq for StaticBacktrace {}

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
                        if !name.starts_with("backtrace::") && !name.ends_with("::track_allocated")
                        {
                            symbols.push(pprof::Symbol {
                                name: Some(name.as_bytes().to_vec()),
                                addr: None,
                                lineno: None,
                                filename: None,
                            })
                        }
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
    // println!("allocated {}", size);

    let mut bt = StaticBacktrace::new();
    backtrace::trace(|frame| bt.push(frame));

    let mut profiler = HEAP_PROFILER.write();
    profiler.collector.add(bt, size as isize).unwrap();
}

pub fn demo() {
    let profiler = HEAP_PROFILER.read();

    println!("DEMO");
    let mut data: HashMap<pprof::Frames, isize> = HashMap::new();

    for entry in profiler.collector.try_iter().unwrap() {
        data.insert(entry.item.clone().into(), entry.count);
    }
    let report = pprof::Report { data };

    let filename = "/tmp/memflame.svg";
    println!("Writing to {}", filename);
    let mut file = std::fs::File::create(filename).unwrap();
    let mut options: pprof::flamegraph::Options = Default::default();

    options.count_name = "bytes".to_string();
    options.colors =
        pprof::flamegraph::color::Palette::Basic(pprof::flamegraph::color::BasicPalette::Mem);

    report
        .flamegraph_with_options(&mut file, &mut options)
        .unwrap();

    let mut buf = vec![];
    report.pprof().unwrap().encode(&mut buf).unwrap();
    let filename = "/tmp/memflame.pb";
    println!("Writing to {}", filename);
    let mut file = std::fs::File::create(filename).unwrap();
    file.write_all(&buf).unwrap();
}
