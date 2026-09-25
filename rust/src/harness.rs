//! Producer/consumer harness, a port of `include/producer_consumer.h`.
//!
//! Producer `p` pushes every value `j` in `0..n_items` with
//! `j % n_producers == p`, so across all producers the queue carries each
//! value exactly once. That gives a cheap, exact correctness check: the
//! consumed values must sum to `n_items * (n_items - 1) / 2`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Instant;

use crate::BoundedQueue;

#[derive(Debug, Clone, Copy)]
pub struct Item {
    pub producer_id: usize,
    pub value: u64,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RunResult {
    pub elapsed_seconds: f64,
    pub items_produced: u64,
    pub items_consumed: u64,
    pub consumed_value_sum: u64,
    pub expected_value_sum: u64,
    pub verified: bool,
}

impl RunResult {
    pub fn throughput_items_per_sec(&self) -> f64 {
        if self.elapsed_seconds > 0.0 {
            self.items_consumed as f64 / self.elapsed_seconds
        } else {
            0.0
        }
    }
}

/// Runs one full producer/consumer cycle against queue type `Q`.
///
/// `thread::scope` lets every thread borrow the queue directly. The C++
/// version passes `std::ref(queue)` and relies on the programmer to join
/// every thread before the queue goes out of scope; here the compiler
/// rejects the program if a thread could outlive the queue.
pub fn run_once<Q: BoundedQueue<Item>>(
    n_items: u64,
    n_producers: usize,
    n_consumers: usize,
    buffer_size: usize,
) -> RunResult {
    assert!(n_producers > 0 && n_consumers > 0);
    let queue = Q::with_capacity(buffer_size);
    let consumed_count = AtomicU64::new(0);
    let consumed_sum = AtomicU64::new(0);

    let start = Instant::now();
    thread::scope(|s| {
        let producers: Vec<_> = (0..n_producers)
            .map(|p| {
                let queue = &queue;
                s.spawn(move || {
                    let mut j = p as u64;
                    while j < n_items {
                        queue.push(Item {
                            producer_id: p,
                            value: j,
                        });
                        j += n_producers as u64;
                    }
                })
            })
            .collect();

        for _ in 0..n_consumers {
            let (queue, consumed_count, consumed_sum) = (&queue, &consumed_count, &consumed_sum);
            s.spawn(move || {
                while let Some(item) = queue.pop() {
                    consumed_count.fetch_add(1, Ordering::Relaxed);
                    consumed_sum.fetch_add(item.value, Ordering::Relaxed);
                }
            });
        }

        for handle in producers {
            handle.join().unwrap();
        }
        // No more items are coming; wake any consumers waiting on an empty
        // queue so they can drain it and exit.
        queue.shutdown();
    });
    let elapsed = start.elapsed().as_secs_f64();

    let items_consumed = consumed_count.load(Ordering::Relaxed);
    let consumed_value_sum = consumed_sum.load(Ordering::Relaxed);
    let expected_value_sum = n_items * n_items.saturating_sub(1) / 2;

    RunResult {
        elapsed_seconds: elapsed,
        items_produced: n_items,
        items_consumed,
        consumed_value_sum,
        expected_value_sum,
        verified: items_consumed == n_items && consumed_value_sum == expected_value_sum,
    }
}
