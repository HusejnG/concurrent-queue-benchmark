//! Bounded lock-free MPMC queue, a port of `include/lockfree_queue.h`.
//!
//! Same algorithm as the C++ version (Dmitry Vyukov's ring buffer): every
//! slot carries its own sequence number, and producers and consumers claim
//! slots with a compare-and-swap on a shared position counter. The memory
//! orderings are identical too, because Rust atomics use the C++20 memory
//! model.
//!
//! What changes in Rust is what the compiler checks. The C++ version is
//! only correct if every caller uses it correctly; here the type system
//! enforces that values are moved in and out exactly once, and the
//! `unsafe` code is confined to the two places where the algorithm itself,
//! not the compiler, guarantees exclusive access to a slot.

use std::cmp::Ordering as Cmp;
use std::mem::MaybeUninit;

use crate::sync::{yield_now, AtomicBool, AtomicUsize, Ordering, UnsafeCell};

/// Aligns its contents to a 64-byte cache line so that the producer and
/// consumer counters never share a line (the C++ version uses `alignas(64)`
/// for the same reason).
#[repr(align(64))]
struct CachePadded<T>(T);

struct Slot<T> {
    sequence: AtomicUsize,
    value: UnsafeCell<MaybeUninit<T>>,
}

pub struct LockFreeQueue<T> {
    buffer: Box<[Slot<T>]>,
    capacity: usize,
    enqueue_pos: CachePadded<AtomicUsize>,
    dequeue_pos: CachePadded<AtomicUsize>,
    done: CachePadded<AtomicBool>,
}

// SAFETY: the queue never hands out shared references to a stored `T`;
// values are only moved in by one producer and moved out by one consumer,
// with the slot's sequence number (release/acquire) ordering the two.
// Moving a `T` across threads is exactly what `T: Send` permits, so
// `T: Send` is sufficient for both `Send` and `Sync`. `T: Sync` is not
// required, because two threads never observe the same `T` at once.
unsafe impl<T: Send> Send for LockFreeQueue<T> {}
unsafe impl<T: Send> Sync for LockFreeQueue<T> {}

impl<T> LockFreeQueue<T> {
    /// Creates a queue that holds up to `capacity` items.
    ///
    /// # Panics
    /// Panics if `capacity` is less than 2. A slot is marked "filled" with
    /// sequence `pos + 1` and "free for the next lap" with
    /// `pos + capacity`; with a capacity of 1 those are the same value, so
    /// a producer could not tell a full slot from an empty one.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity >= 2, "capacity must be at least 2");
        let buffer = (0..capacity)
            .map(|i| Slot {
                sequence: AtomicUsize::new(i),
                value: UnsafeCell::new(MaybeUninit::uninit()),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            buffer,
            capacity,
            enqueue_pos: CachePadded(AtomicUsize::new(0)),
            dequeue_pos: CachePadded(AtomicUsize::new(0)),
            done: CachePadded(AtomicBool::new(false)),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Non-blocking push. If the queue is full, the value is handed back
    /// in `Err` so the caller keeps ownership of it.
    pub fn try_push(&self, value: T) -> Result<(), T> {
        let mut pos = self.enqueue_pos.0.load(Ordering::Relaxed);
        loop {
            let slot = &self.buffer[pos % self.capacity];
            let seq = slot.sequence.load(Ordering::Acquire);
            // Modular difference, so the comparison stays correct even if
            // the position counters ever wrap around.
            let diff = seq.wrapping_sub(pos) as isize;

            match diff.cmp(&0) {
                Cmp::Equal => match self.enqueue_pos.0.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // SAFETY: winning the CAS for `pos` gives this thread
                        // exclusive ownership of the slot until it publishes
                        // it below. No consumer reads the slot before the
                        // sequence becomes `pos + 1`.
                        slot.value.with_mut(|p| unsafe { (*p).write(value) });
                        slot.sequence.store(pos.wrapping_add(1), Ordering::Release);
                        return Ok(());
                    }
                    // Unlike C++, where a failed CAS silently overwrites
                    // `pos`, Rust returns the current value explicitly.
                    Err(actual) => pos = actual,
                },
                Cmp::Less => return Err(value), // full
                Cmp::Greater => pos = self.enqueue_pos.0.load(Ordering::Relaxed),
            }
        }
    }

    /// Non-blocking pop. Returns `None` if the queue is empty.
    pub fn try_pop(&self) -> Option<T> {
        let mut pos = self.dequeue_pos.0.load(Ordering::Relaxed);
        loop {
            let slot = &self.buffer[pos % self.capacity];
            let seq = slot.sequence.load(Ordering::Acquire);
            let diff = seq.wrapping_sub(pos.wrapping_add(1)) as isize;

            match diff.cmp(&0) {
                Cmp::Equal => match self.dequeue_pos.0.compare_exchange_weak(
                    pos,
                    pos.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        // SAFETY: the Acquire load above observed the
                        // producer's Release store of `pos + 1`, so the
                        // value is fully written, and winning the CAS gives
                        // this thread exclusive ownership of it. The value
                        // is moved out exactly once; the slot is then handed
                        // back to producers for the next lap.
                        let value = slot.value.with(|p| unsafe { (*p).assume_init_read() });
                        slot.sequence
                            .store(pos.wrapping_add(self.capacity), Ordering::Release);
                        return Some(value);
                    }
                    Err(actual) => pos = actual,
                },
                Cmp::Less => return None, // empty
                Cmp::Greater => pos = self.dequeue_pos.0.load(Ordering::Relaxed),
            }
        }
    }

    /// Busy-waiting push, used by the benchmark as a drop-in for the
    /// blocking queue's `push`.
    pub fn push(&self, mut value: T) {
        loop {
            match self.try_push(value) {
                Ok(()) => return,
                Err(v) => {
                    value = v;
                    yield_now();
                }
            }
        }
    }

    /// Busy-waiting pop. Returns `None` only after `shutdown()` has been
    /// called and the queue has been drained.
    pub fn pop(&self) -> Option<T> {
        loop {
            if let Some(value) = self.try_pop() {
                return Some(value);
            }
            if self.done.0.load(Ordering::Acquire) {
                // One last attempt, in case an item landed between the
                // failed try_pop above and the shutdown check.
                return self.try_pop();
            }
            yield_now();
        }
    }

    /// Signals that no more items will be pushed.
    pub fn shutdown(&self) {
        self.done.0.store(true, Ordering::Release);
    }
}

impl<T> Drop for LockFreeQueue<T> {
    // The C++ version stores plain `T` in every slot, so it gets
    // destruction for free but also requires `T` to be default
    // constructible. Here slots hold `MaybeUninit<T>`, so anything still
    // queued must be dropped by hand to avoid leaking it.
    fn drop(&mut self) {
        while self.try_pop().is_some() {}
    }
}
