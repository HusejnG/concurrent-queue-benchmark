//! Rust port of the mutex vs. lock-free MPMC queue benchmark.
//!
//! The C++ original lives in `../include`. Both implementations follow the
//! same designs, so the benchmark compares the languages on the same
//! algorithm.

mod sync;

pub mod lockfree_queue;
pub use lockfree_queue::LockFreeQueue;

#[cfg(not(loom))]
pub mod harness;
#[cfg(not(loom))]
pub mod mutex_queue;
#[cfg(not(loom))]
pub use mutex_queue::MutexQueue;

/// Common interface used by the benchmark harness, mirroring the implicit
/// template interface in the C++ `producer_consumer.h`.
pub trait BoundedQueue<T>: Sync {
    fn with_capacity(capacity: usize) -> Self
    where
        Self: Sized;
    fn push(&self, value: T);
    fn pop(&self) -> Option<T>;
    fn shutdown(&self);
}

impl<T: Send> BoundedQueue<T> for LockFreeQueue<T> {
    fn with_capacity(capacity: usize) -> Self {
        LockFreeQueue::new(capacity)
    }
    fn push(&self, value: T) {
        LockFreeQueue::push(self, value)
    }
    fn pop(&self) -> Option<T> {
        LockFreeQueue::pop(self)
    }
    fn shutdown(&self) {
        LockFreeQueue::shutdown(self)
    }
}

#[cfg(not(loom))]
impl<T: Send> BoundedQueue<T> for MutexQueue<T> {
    fn with_capacity(capacity: usize) -> Self {
        MutexQueue::new(capacity)
    }
    fn push(&self, value: T) {
        MutexQueue::push(self, value)
    }
    fn pop(&self) -> Option<T> {
        MutexQueue::pop(self)
    }
    fn shutdown(&self) {
        MutexQueue::shutdown(self)
    }
}
