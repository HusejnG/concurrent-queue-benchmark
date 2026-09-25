//! Synchronization primitives used by the lock-free queue.
//!
//! Under a normal build these are the std types. Under `--cfg loom` they
//! become loom's instrumented versions, which lets the loom test suite
//! explore every interleaving of the queue's atomic operations without
//! changing a single line of the queue itself.

#[cfg(loom)]
pub(crate) use loom::cell::UnsafeCell;
#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(loom)]
pub(crate) use loom::thread::yield_now;

#[cfg(not(loom))]
pub(crate) use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(not(loom))]
pub(crate) use std::thread::yield_now;

/// Thin wrapper over `std::cell::UnsafeCell` that exposes the same
/// closure-based API as `loom::cell::UnsafeCell`, so the queue can be
/// written once against a single interface.
#[cfg(not(loom))]
#[derive(Debug)]
pub(crate) struct UnsafeCell<T>(std::cell::UnsafeCell<T>);

#[cfg(not(loom))]
impl<T> UnsafeCell<T> {
    pub(crate) fn new(value: T) -> Self {
        Self(std::cell::UnsafeCell::new(value))
    }

    pub(crate) fn with<R>(&self, f: impl FnOnce(*const T) -> R) -> R {
        f(self.0.get())
    }

    pub(crate) fn with_mut<R>(&self, f: impl FnOnce(*mut T) -> R) -> R {
        f(self.0.get())
    }
}
