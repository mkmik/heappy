#![allow(dead_code)]

use backtrace::Frame;
use spin::RwLock;
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "jemalloc_shim")]
mod jemalloc_shim;

// On linux you need to reference at least one symbol in a module if we want it be be actually linked.
// Otherwise the hooks like `pub unsafe extern "C" fn malloc(size: size_t) -> *mut c_void` defined in the shim
// module won't override the respective weak symbols from libc, since they don't ever get linked in the final executable.
// On macos this is not necessary, but it doesn't hurt.
// (e.g. the functions that override weak symbols exported by libc)
#[cfg(feature = "jemalloc_shim")]
pub fn dummy_force_link() {
    jemalloc_shim::dummy_force_link();
}

const MAX_DEPTH: usize = 32;

static HEAP_PROFILER_ENABLED: AtomicBool = AtomicBool::new(false);

lazy_static::lazy_static! {
    static ref HEAP_PROFILER_STATE: RwLock<ProfilerState<MAX_DEPTH>> = RwLock::new(ProfilerState::new(1));
}

/// RAII structure used to stop profiling when dropped. It is the only interface to access the heap profiler.
pub struct HeapProfilerGuard {}

impl HeapProfilerGuard {
    pub fn new(period: usize) -> Self {
        Profiler::start(period);
        Self {}
    }

    pub fn report(self) -> HeapReport {
        std::mem::drop(self);
        HeapReport::new()
    }
}

impl Drop for HeapProfilerGuard {
    fn drop(&mut self) {
        Profiler::stop();
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

    fn start(period: usize) {
        let mut profiler = HEAP_PROFILER_STATE.write();
        *profiler = ProfilerState::new(period);
        std::mem::drop(profiler);

        Self::set_enabled(true);
    }

    fn stop() {
        Self::set_enabled(false);
    }

    // Called by malloc hooks to record a memory allocation event.
    pub(crate) unsafe fn track_allocated(size: isize) {
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
                // TODO: track deallocations
                if size <= 0 {
                    return;
                }

                let mut profiler = HEAP_PROFILER_STATE.write();
                profiler.allocated_objects += 1;
                profiler.allocated_bytes += size;

                if profiler.allocated_bytes >= profiler.next_sample {
                    profiler.next_sample = profiler.allocated_bytes + profiler.period as isize;
                    let mut bt = Frames::new();
                    backtrace::trace(|frame| bt.push(frame));

                    profiler.collector.add(bt, size).unwrap();
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct HeapReport {
    report: pprof::Report,
    period: usize,
}

impl HeapReport {
    fn new() -> Self {
        let profiler = HEAP_PROFILER_STATE.read();

        let mut data: HashMap<pprof::Frames, isize> = HashMap::new();

        for entry in profiler.collector.try_iter().unwrap() {
            data.insert(entry.item.clone().into(), entry.count);
        }
        let report = pprof::Report { data };
        Self {
            report,
            period: profiler.period,
        }
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
        proto.string_table.push("alloc_space".to_string());
        proto.string_table.push("bytes".to_string());

        let sample_type = pprof::protos::ValueType {
            r#type: type_idx as i64,
            unit: unit_idx as i64,
        };
        proto.sample_type = vec![sample_type];

        let period_type_idx = proto.string_table.len();
        proto.string_table.push("space".to_string());
        proto.period_type = Some(pprof::protos::ValueType {
            r#type: period_type_idx as i64,
            unit: unit_idx as i64,
        });
        proto.period = self.period as i64;

        let drop_frames_idx = proto.string_table.len();
        proto
            .string_table
            .push(".*::Profiler::track_allocated".to_string());
        proto.drop_frames = drop_frames_idx as i64;

        proto
    }
}

// Current profiler state, collection of sampled frames.
struct ProfilerState<const N: usize> {
    collector: pprof::Collector<Frames<N>>,
    allocated_objects: isize,
    allocated_bytes: isize,
    // take a sample when allocated crosses this threshold
    next_sample: isize,
    // take a sample every period bytes.
    period: usize,
}

impl<const N: usize> ProfilerState<N> {
    fn new(period: usize) -> Self {
        Self {
            collector: pprof::Collector::new().unwrap(),
            period,
            allocated_objects: 0,
            allocated_bytes: 0,
            next_sample: period as isize,
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
                let mut symbols = Vec::new();
                backtrace::resolve_frame(frame, |symbol| {
                    if let Some(name) = symbol.name() {
                        let name = format!("{:#}", name);
                        if !name.starts_with("alloc::alloc::")
                            && name != "<alloc::alloc::Global as core::alloc::Allocator>::allocate"
                        {
                            symbols.push(symbol.into());
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
