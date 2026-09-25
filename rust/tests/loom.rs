//! Model-checked concurrency tests.
//!
//! Run with:
//!   RUSTFLAGS="--cfg loom" cargo test --release --test loom
//!
//! loom runs each closure many times, systematically exploring the
//! possible interleavings of the queue's atomic operations and checking
//! the C++/Rust memory model on every one. A bug that shows up once in a
//! million ordinary test runs shows up here deterministically.

#![cfg(loom)]

use cq_rust::LockFreeQueue;
use loom::sync::Arc;
use loom::thread;

/// Pops one item, yielding to loom's scheduler while the queue is empty.
fn pop_blocking(q: &LockFreeQueue<u32>) -> u32 {
    loop {
        if let Some(v) = q.try_pop() {
            return v;
        }
        thread::yield_now();
    }
}

#[test]
fn two_producers_one_consumer_lose_nothing() {
    loom::model(|| {
        let q = Arc::new(LockFreeQueue::new(2));
        let producers: Vec<_> = [1u32, 2]
            .into_iter()
            .map(|v| {
                let q = q.clone();
                thread::spawn(move || q.push(v))
            })
            .collect();

        let sum = pop_blocking(&q) + pop_blocking(&q);
        for p in producers {
            p.join().unwrap();
        }
        assert_eq!(sum, 3);
    });
}

#[test]
fn two_consumers_never_take_the_same_item() {
    loom::model(|| {
        let q = Arc::new(LockFreeQueue::new(2));
        q.try_push(7u32).unwrap();

        let consumers: Vec<_> = (0..2)
            .map(|_| {
                let q = q.clone();
                thread::spawn(move || q.try_pop())
            })
            .collect();
        let taken: Vec<_> = consumers
            .into_iter()
            .map(|c| c.join().unwrap())
            .flatten()
            .collect();

        assert_eq!(taken, vec![7]);
    });
}

#[test]
fn single_producer_order_survives_wraparound() {
    loom::model(|| {
        let q = Arc::new(LockFreeQueue::new(2));
        let producer = {
            let q = q.clone();
            thread::spawn(move || {
                for i in 0..3u32 {
                    q.push(i);
                }
            })
        };

        let got = [pop_blocking(&q), pop_blocking(&q), pop_blocking(&q)];
        producer.join().unwrap();
        assert_eq!(got, [0, 1, 2]);
    });
}
