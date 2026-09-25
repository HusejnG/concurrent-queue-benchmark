#![cfg(not(loom))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use cq_rust::harness::{run_once, Item};
use cq_rust::{LockFreeQueue, MutexQueue};

// ---- lock-free queue, single-threaded behaviour ---------------------------

#[test]
fn lockfree_preserves_fifo_order() {
    let q = LockFreeQueue::new(8);
    for i in 0..8 {
        q.try_push(i).unwrap();
    }
    for i in 0..8 {
        assert_eq!(q.try_pop(), Some(i));
    }
}

#[test]
fn lockfree_full_queue_returns_the_value() {
    let q = LockFreeQueue::new(2);
    q.try_push(1).unwrap();
    q.try_push(2).unwrap();
    // The rejected value comes back to the caller instead of being lost.
    assert_eq!(q.try_push(3), Err(3));
}

#[test]
fn lockfree_empty_queue_returns_none() {
    let q: LockFreeQueue<u32> = LockFreeQueue::new(4);
    assert_eq!(q.try_pop(), None);
}

#[test]
fn lockfree_wraps_around_many_laps() {
    // Capacity 3 is deliberately not a power of two, and 10_000 items means
    // every slot's sequence number goes around thousands of times.
    let q = LockFreeQueue::new(3);
    for i in 0..10_000u32 {
        q.try_push(i).unwrap();
        assert_eq!(q.try_pop(), Some(i));
    }
}

// Capacity 1 is rejected: with a single slot the "filled" and "free for
// the next lap" sequence numbers collide, and a second push would silently
// overwrite an unread item. This test is what exposed the same bug in the
// original C++ implementation.
#[test]
#[should_panic(expected = "capacity must be at least 2")]
fn lockfree_rejects_capacity_one() {
    let _q: LockFreeQueue<char> = LockFreeQueue::new(1);
}

#[test]
fn lockfree_capacity_two_never_overwrites() {
    let q = LockFreeQueue::new(2);
    for lap in 0..1000u32 {
        q.try_push(lap).unwrap();
        q.try_push(lap + 1).unwrap();
        assert_eq!(q.try_push(u32::MAX), Err(u32::MAX));
        assert_eq!(q.try_pop(), Some(lap));
        assert_eq!(q.try_pop(), Some(lap + 1));
    }
}

#[test]
fn lockfree_moves_non_copy_values() {
    // The C++ version copies `T` in and out; here values are moved, so heap
    // types like String pass through without being cloned.
    let q = LockFreeQueue::new(4);
    q.try_push(String::from("telemetry")).unwrap();
    assert_eq!(q.try_pop().as_deref(), Some("telemetry"));
}

#[test]
#[should_panic(expected = "capacity must be at least 2")]
fn lockfree_zero_capacity_panics() {
    let _q: LockFreeQueue<u8> = LockFreeQueue::new(0);
}

/// Counts how many instances have been dropped.
struct DropCounter(Arc<AtomicUsize>);
impl Drop for DropCounter {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn lockfree_drops_items_left_in_queue() {
    let drops = Arc::new(AtomicUsize::new(0));
    {
        let q = LockFreeQueue::new(8);
        for _ in 0..5 {
            q.try_push(DropCounter(drops.clone())).ok().unwrap();
        }
        drop(q.try_pop()); // one popped and dropped by the caller
    }
    // The remaining four must be dropped by the queue itself, not leaked.
    assert_eq!(drops.load(Ordering::SeqCst), 5);
}

#[test]
fn lockfree_pop_returns_none_after_shutdown_and_drain() {
    let q = LockFreeQueue::new(4);
    q.push(1);
    q.shutdown();
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), None);
}

// ---- mutex queue ----------------------------------------------------------

#[test]
fn mutex_preserves_fifo_order() {
    let q = MutexQueue::new(4);
    for i in 0..4 {
        q.push(i);
    }
    for i in 0..4 {
        assert_eq!(q.pop(), Some(i));
    }
}

#[test]
fn mutex_shutdown_wakes_blocked_consumer() {
    let q: Arc<MutexQueue<u32>> = Arc::new(MutexQueue::new(4));
    let consumer = {
        let q = q.clone();
        thread::spawn(move || q.pop())
    };
    // The consumer is blocked on an empty queue; shutdown must release it.
    thread::sleep(std::time::Duration::from_millis(50));
    q.shutdown();
    assert_eq!(consumer.join().unwrap(), None);
}

// ---- full multi-producer / multi-consumer runs ----------------------------

fn assert_verified<Q: cq_rust::BoundedQueue<Item>>(p: usize, c: usize, buffer: usize) {
    let r = run_once::<Q>(200_000, p, c, buffer);
    assert!(
        r.verified,
        "{p}x{c}: consumed {} of {}, sum {} vs expected {}",
        r.items_consumed, r.items_produced, r.consumed_value_sum, r.expected_value_sum
    );
}

#[test]
fn lockfree_mpmc_runs_are_exact() {
    for (p, c) in [(1, 1), (4, 4), (8, 8), (2, 6)] {
        assert_verified::<LockFreeQueue<Item>>(p, c, 64);
    }
}

#[test]
fn mutex_mpmc_runs_are_exact() {
    for (p, c) in [(1, 1), (4, 4), (8, 8), (6, 2)] {
        assert_verified::<MutexQueue<Item>>(p, c, 64);
    }
}

#[test]
fn lockfree_tiny_buffer_under_contention() {
    // A two-slot buffer with 8 producers and 8 consumers forces constant
    // full/empty transitions, which is where ordering bugs show up.
    assert_verified::<LockFreeQueue<Item>>(8, 8, 2);
}
