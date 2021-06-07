use backtrace::Frame;
use spin::RwLock;
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(any(target_os = "macos", feature = "jemalloc_shim"))]
mod jemalloc_shim;

const MAX_DEPTH: usize = 32;

static HEAP_PROFILER_ENABLED: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref HEAP_PROFILER_STATE: RwLock<ProfilerState<MAX_DEPTH>> = RwLock::new(ProfilerState::new());
}

/// RAII structure used to stop profiling when dropped. It is the only interface to access the heap profiler.
pub struct HeapProfilerGuard {}

impl HeapProfilerGuard {
    pub fn new() -> Self {
        Profiler::set_enabled(true);
        Self {}
    }

    pub fn report(self) -> HeapReport {
        std::mem::drop(self);
        HeapReport::new()
    }
}

impl Default for HeapProfilerGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HeapProfilerGuard {
    fn drop(&mut self) {
        Profiler::set_enabled(false);
    }
}

struct Profiler;

impl Profiler {
    fn enabled() -> bool {
        HEAP_PROFILER_ENABLED.load(Ordering::SeqCst)
    }

    fn set_enabled(value: bool) {
        HEAP_PROFILER_ENABLED.store(value, Ordering::SeqCst)
    }

    // Called by malloc hooks to record a memory allocation event.
    pub(crate) unsafe fn track_allocated(size: usize) {
        thread_local!(static ENTERED: Cell<bool> = Cell::new(false));

        struct ResetOnDrop;

        impl Drop for ResetOnDrop {
            fn drop(&mut self) {
                ENTERED.with(|b| b.set(false));
            }
        }

        if !ENTERED.with(|b| b.replace(true)) {
            let _reset_on_drop = ResetOnDrop;
            if Self::enabled() {
                let mut bt = Frames::new();
                backtrace::trace(|frame| bt.push(frame));

                let mut profiler = HEAP_PROFILER_STATE.write();
                profiler.collector.add(bt, size as isize).unwrap();
            }
        }
    }
}

#[derive(Debug)]
pub struct HeapReport {
    report: pprof::Report,
}

impl HeapReport {
    fn new() -> Self {
        let profiler = HEAP_PROFILER_STATE.read();

        let mut data: HashMap<pprof::Frames, isize> = HashMap::new();

        for entry in profiler.collector.try_iter().unwrap() {
            data.insert(entry.item.clone().into(), entry.count);
        }
        let report = pprof::Report { data };
        Self { report }
    }

    /// flamegraph will write an svg flamegraph into writer.
    pub fn flamegraph<W>(&self, writer: W)
    where
        W: Write,
    {
        let mut options: pprof::flamegraph::Options = Default::default();

        options.count_name = "bytes".to_string();
        options.colors =
            pprof::flamegraph::color::Palette::Basic(pprof::flamegraph::color::BasicPalette::Mem);

        self.report
            .flamegraph_with_options(writer, &mut options)
            .unwrap();
    }

    /// produce a pprof proto (for use with go tool pprof and compatible visualizers)
    pub fn pprof(&self) -> pprof::protos::Profile {
        // The pprof crate currently only supports sampling cpu.
        // But other than that it does exactly the work we need, so instead of
        // duplicating the pprof proto generation code here, we're just fixing up the "legend".
        // There is work underway to add this natively to pprof-rs https://github.com/tikv/pprof-rs/pull/45
        let mut proto = self.report.pprof().unwrap();
        let (type_idx, unit_idx) = (proto.string_table.len(), proto.string_table.len() + 1);
        proto.string_table.push("space".to_owned());
        proto.string_table.push("bytes".to_owned());
        let sample_type = pprof::protos::ValueType {
            r#type: type_idx as i64,
            unit: unit_idx as i64,
        };
        proto.sample_type = vec![sample_type];
        proto.string_table[type_idx] = "space".to_string();
        proto.string_table[unit_idx] = "bytes".to_string();

        proto
    }
}

// Current profiler state, collection of sampled frames.
struct ProfilerState<const N: usize> {
    collector: pprof::Collector<Frames<N>>,
}

impl<const N: usize> ProfilerState<N> {
    fn new() -> Self {
        Self {
            collector: pprof::Collector::new().unwrap(),
        }
    }
}

struct Frames<const N: usize> {
    frames: [Frame; N],
    size: usize,
}

impl<const N: usize> Clone for Frames<N> {
    fn clone(&self) -> Self {
        let mut n = unsafe { Self::new() };
        for i in 0..self.size {
            n.frames[i] = self.frames[i].clone()
        }
        n.size = self.size;
        n
    }
}

impl<const N: usize> Frames<N> {
    unsafe fn new() -> Self {
        Self {
            frames: std::mem::MaybeUninit::uninit().assume_init(),
            size: 0,
        }
    }

    /// Push will push up to N frames in the frames array.
    unsafe fn push(&mut self, frame: &Frame) -> bool {
        self.frames[self.size] = frame.clone();
        self.size += 1;
        self.size < N
    }

    fn iter(&self) -> FramesIterator<N> {
        FramesIterator(self, 0)
    }
}

impl<const N: usize> Hash for Frames<N> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.iter()
            .for_each(|frame| frame.symbol_address().hash(state));
    }
}

impl<const N: usize> PartialEq for Frames<N> {
    fn eq(&self, other: &Self) -> bool {
        Iterator::zip(self.iter(), other.iter())
            .map(|(s1, s2)| s1.symbol_address() == s2.symbol_address())
            .all(|equal| equal)
    }
}

impl<const N: usize> Eq for Frames<N> {}

struct FramesIterator<'a, const N: usize>(&'a Frames<N>, usize);

impl<'a, const N: usize> Iterator for FramesIterator<'a, N> {
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

impl<const N: usize> From<Frames<N>> for pprof::Frames {
    fn from(bt: Frames<N>) -> Self {
        let frames = bt
            .iter()
            .map(|frame| {
                let mut symbols: Vec<pprof::Symbol> = Vec::new();
                backtrace::resolve_frame(frame, |symbol| {
                    if let Some(name) = symbol.name() {
                        let name = format!("{:#}", name);
                        if !name.starts_with("backtrace::")
                            && !name.ends_with("::track_allocated")
                            && !name.starts_with("alloc::alloc::")
                            && name != "<alloc::alloc::Global as core::alloc::Allocator>::allocate"
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
