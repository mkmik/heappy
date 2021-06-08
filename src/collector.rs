use core::cmp::Eq;
use core::default::Default;
use core::hash::Hash;
use std::collections::HashMap;

#[derive(Default)]
pub struct MemProfileRecord {
    pub alloc_bytes: isize,
    pub free_bytes: isize,
    pub alloc_objects: isize,
    pub free_objects: isize,
}

impl MemProfileRecord {
    pub fn in_use_bytes(&self) -> isize {
        self.alloc_bytes - self.free_bytes
    }

    pub fn in_use_objects(&self) -> isize {
        self.alloc_objects - self.free_objects
    }
}

pub struct Collector<K: Hash + Eq + 'static> {
    map: HashMap<K, MemProfileRecord>,
}

impl<K: Hash + Eq + 'static> Collector<K> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn record(&mut self, key: K, bytes: isize) {
        let rec = self.map.entry(key).or_insert_with(Default::default);
        match bytes.cmp(&0) {
            std::cmp::Ordering::Greater => {
                rec.alloc_bytes += bytes;
                rec.alloc_objects += 1;
            }
            std::cmp::Ordering::Less => {
                rec.free_bytes += -bytes;
                rec.free_objects += 1;
            }
            std::cmp::Ordering::Equal => {
                // ignore
            }
        }
    }

    pub fn into_iter(self) -> impl Iterator<Item = (K, MemProfileRecord)> {
        self.map.into_iter()
    }
}

impl<K: Hash + Eq + 'static> Default for Collector<K> {
    fn default() -> Self {
        Self::new()
    }
}
