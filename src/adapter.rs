use libc::c_void;

pub use crate::jemalloc_adapter::*;
use crate::profiler::{Frames, MAX_DEPTH};
use crate::Backtracer;

#[cfg(feature = "magic_number")]
const MAGIC: u64 = 0xFEEDDEADBEEFF00D;

pub struct Block {
    block_base: *mut c_void,
}

impl Block {
    pub unsafe fn new(ptr: *mut c_void) -> Self {
        let header = BlockHeader {
            size: 0,
            frames: None,
            #[cfg(feature = "magic_number")]
            magic: MAGIC,
        };
        std::ptr::write(ptr as *mut BlockHeader, header);
        Self { block_base: ptr }
    }

    pub unsafe fn adopt(body: *mut c_void) -> Self {
        let ptr = body.sub(Self::header_size());
        Self { block_base: ptr }
    }

    pub unsafe fn rebase(&mut self, ptr: *mut c_void) {
        self.block_base = ptr;
    }

    pub unsafe fn set_size(&mut self, size: usize) -> usize {
        let header = self.block_base as *mut BlockHeader;
        let old_size = (*header).size;
        (*header).size = size;
        old_size
    }

    pub unsafe fn size(&self) -> usize {
        let header = self.block_base as *const BlockHeader;
        (*header).size
    }

    pub unsafe fn user_payload(&self) -> *mut c_void {
        self.block_base.add(Self::header_size())
    }

    pub fn header_size() -> usize {
        std::mem::size_of::<BlockHeader>()
    }

    pub unsafe fn ptr(&self) -> *mut c_void {
        self.block_base
    }

    #[cfg(feature = "magic_number")]
    pub unsafe fn check(&self) -> bool {
        let header = self.block_base as *const BlockHeader;
        #[cfg(feature = "assert_magic_number")]
        {
            assert_eq!((*header).magic, MAGIC);
        }
        (*header).magic == MAGIC
    }

    #[cfg(not(feature = "magic_number"))]
    pub unsafe fn check(&self) -> bool {
        true
    }

    pub fn free(&self) {
        unsafe {
            let header = self.block_base as *mut BlockHeader;
            std::ptr::drop_in_place(header);
        }
    }
}

impl Backtracer for Block {
    fn backtrace(&mut self, create_if_missing: bool) -> Frames<MAX_DEPTH> {
        unsafe {
            let frames = &mut (*(self.block_base as *mut BlockHeader)).frames;
            if frames.is_none() {
                if !create_if_missing {
                    return Frames::new();
                }
                frames.insert({
                    let mut bt = Frames::new();
                    // we're already holding a lock when calling this.
                    backtrace::trace_unsynchronized(|frame| bt.push(frame));
                    Box::new(bt)
                });
            };
            *frames.as_ref().unwrap().clone()
        }
    }
}

// minimum alignment for x86; figure out how to make this cross platform
#[repr(C, align(16))]
struct BlockHeader {
    size: usize,
    frames: Option<Box<Frames<MAX_DEPTH>>>,
    #[cfg(feature = "magic_number")]
    magic: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block() {
        unsafe {
            const SIZE: usize = 42;
            let ptr = sys_malloc(SIZE);

            let mut block = Block::new(ptr);
            block.set_size(SIZE);

            assert_eq!(block.size(), SIZE);

            let body = block.user_payload();

            let adopted = Block::adopt(body);
            assert_eq!(adopted.block_base, block.block_base);
            assert_eq!(adopted.size(), SIZE);

            sys_free(block.block_base);
        }
    }
}
