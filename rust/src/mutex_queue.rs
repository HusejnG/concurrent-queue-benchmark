//! Bounded blocking MPMC queue, a port of `include/blocking_queue.h`.
//!
//! One mutex guards the buffer and the shutdown flag; two condition
//! variables let producers wait while the queue is full and consumers wait
//! while it is empty.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

struct Inner<T> {
    items: VecDeque<T>,
    done: bool,
}

pub struct MutexQueue<T> {
    inner: Mutex<Inner<T>>,
    not_full: Condvar,
    not_empty: Condvar,
    capacity: usize,
}

impl<T> MutexQueue<T> {
    /// # Panics
    /// Panics if `capacity` is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be greater than zero");
        Self {
            inner: Mutex::new(Inner {
                items: VecDeque::with_capacity(capacity),
                done: false,
            }),
            not_full: Condvar::new(),
            not_empty: Condvar::new(),
            capacity,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Blocks while the queue is full.
    pub fn push(&self, value: T) {
        let mut inner = self.inner.lock().unwrap();
        while inner.items.len() >= self.capacity && !inner.done {
            inner = self.not_full.wait(inner).unwrap();
        }
        inner.items.push_back(value);
        // Release the lock before notifying so the woken consumer does not
        // immediately block on a mutex this thread still holds.
        drop(inner);
        self.not_empty.notify_one();
    }

    /// Blocks while the queue is empty. Returns `None` once the queue is
    /// empty and `shutdown()` has been called.
    pub fn pop(&self) -> Option<T> {
        let mut inner = self.inner.lock().unwrap();
        while inner.items.is_empty() && !inner.done {
            inner = self.not_empty.wait(inner).unwrap();
        }
        let value = inner.items.pop_front();
        drop(inner);
        if value.is_some() {
            self.not_full.notify_one();
        }
        value
    }

    /// Signals that no more items will be pushed and wakes every blocked
    /// thread.
    pub fn shutdown(&self) {
        self.inner.lock().unwrap().done = true;
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }
}
