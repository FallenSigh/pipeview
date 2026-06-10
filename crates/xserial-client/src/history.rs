use crate::event::DecodedEntry;
use std::collections::VecDeque;

pub struct RingBuffer<T> {
    buf: VecDeque<T>,
    limit: usize,
}

impl<T> RingBuffer<T> {
    pub fn new(limit: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(limit.min(1024)),
            limit,
        }
    }
    pub fn push(&mut self, item: T) {
        if self.buf.len() >= self.limit {
            self.buf.pop_front();
        }
        self.buf.push_back(item);
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter()
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        self.buf.get(index)
    }
    pub fn clear(&mut self) {
        self.buf.clear();
    }
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
        while self.buf.len() > self.limit {
            self.buf.pop_front();
        }
    }
    pub fn drain_recent(&self, count: usize) -> Vec<T>
    where
        T: Clone,
    {
        let start = self.buf.len().saturating_sub(count);
        self.buf.iter().skip(start).cloned().collect()
    }
}

impl RingBuffer<DecodedEntry> {
    pub fn entries_for_pipeline(&self, name: &str) -> Vec<&DecodedEntry> {
        self.buf
            .iter()
            .filter(|e| e.pipeline_name == name)
            .collect()
    }
}
