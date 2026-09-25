# Rust port

A Rust implementation of the same two queues as the C++ project one level
up: a mutex + condition variable queue and a lock-free MPMC ring buffer
(Dmitry Vyukov's design). Same algorithm, same benchmark flags, same
output format, so the two languages can be compared on identical work.

## Layout

```
rust/
├── src/
│   ├── lockfree_queue.rs   # port of include/lockfree_queue.h
│   ├── mutex_queue.rs      # port of include/blocking_queue.h
│   ├── harness.rs          # port of include/producer_consumer.h
│   ├── sync.rs             # std vs. loom atomics, selected at compile time
│   ├── lib.rs              # BoundedQueue trait shared by both queues
│   └── main.rs             # port of src/main.cpp (same CLI flags)
└── tests/
    ├── queues.rs           # unit + multi-threaded tests (15)
    └── loom.rs             # model-checked concurrency tests (3)
```

No runtime dependencies. `loom` is only pulled in for `--cfg loom` builds.

## Build, run, test

```bash
cd rust
cargo build --release
cargo run --release -- --nItems 2000000 --nProducers 4 --nConsumers 4 --bufferSize 1024 --mode both
cargo test --release
```

Model checking with [loom](https://github.com/tokio-rs/loom):

```bash
RUSTFLAGS="--cfg loom" LOOM_MAX_PREEMPTIONS=2 cargo test --release --test loom
```

## Results

Same machine and settings as the C++ numbers in the main README: Intel
Core i7-6700HQ (4 cores / 8 threads), Windows, release builds,
`--nItems 2000000`, buffer size 1024. Single runs, so treat small
differences as noise.

| Producers × Consumers | C++ mutex | Rust mutex | C++ lock-free | Rust lock-free |
|---|---|---|---|---|
| 1 × 1 | 6,745,472 | 3,182,211 | 34,431,408 | 35,848,975 |
| 4 × 4 | 3,874,244 | 2,168,340 | 8,273,365 | 8,471,496 |
| 8 × 8 | 4,121,476 | 2,230,078 | 6,287,914 | 5,463,217 |

(items per second)

**Lock-free: the two languages perform the same.** At 1×1 and 4×4 the
Rust version is within 5% of C++, which is what you'd expect from the
same algorithm under the same memory model. The compile-time guarantees
cost nothing measurable on the hot path. At 8×8, sixteen threads share
eight hardware threads and results are the least stable of the three.

**Mutex: the Rust version is roughly half as fast.** The queue logic is
equivalent, so the gap comes from how each standard library implements
`Mutex` and `Condvar` on Windows and the code around them. I haven't
profiled it yet. Because of this, the Rust lock-free-vs-mutex speedup
(11.3x at 1×1) looks larger than the C++ one (5.1x); that ratio says more
about the mutex baseline than about either language, and the lock-free
columns are the fair comparison.

## What stayed the same

The algorithm and the memory orderings are identical to the C++ version.
Rust atomics follow the C++20 memory model, so every
`std::memory_order_acquire` / `release` / `relaxed` in `lockfree_queue.h`
maps one to one onto `Ordering::Acquire` / `Release` / `Relaxed` here. The
producer and consumer counters are still kept on separate cache lines
(`#[repr(align(64))]` instead of `alignas(64)`).

## What the compiler now checks

| | C++ | Rust |
|---|---|---|
| Sharing the queue across threads | Allowed for any `T`; correctness is the caller's job | Only compiles if `T: Send` (`unsafe impl Send/Sync` states exactly why that is enough) |
| Threads outliving the queue | `std::ref(queue)` + manual `join()`; forgetting a join is undefined behaviour | `thread::scope` makes it a compile error |
| Values in slots | Stored as plain `T`, copied in and out; `T` must be default-constructible | Stored as `MaybeUninit<T>` and moved in and out exactly once; works for `String` and other heap types without cloning |
| Rejected push | Returns `false`; the caller still has its copy | Returns `Err(value)`, handing ownership back |
| Items left in the queue at destruction | Destroyed with the buffer | Drained in `Drop`, otherwise they would leak |
| Unsafe code | Everywhere, implicitly | Two small `unsafe` blocks, each with a `SAFETY:` comment explaining why exclusive access is guaranteed |

The lock-free part still needs `unsafe`: the compiler cannot prove that
the sequence-number protocol gives a thread exclusive access to a slot.
What Rust adds is that the unsafe surface is small, named, and justified
in place, while everything around it is checked.

## A bug found in the C++ original

Writing the Rust tests surfaced a bug that exists in both versions.

A slot is marked "filled" with sequence `pos + 1` and "free for the next
lap" with `pos + capacity`. With a capacity of 1 these are the same value,
so a producer cannot tell a full slot from an empty one. A second
`try_push` on a full one-slot queue succeeds and overwrites the unread
item, and the next `try_pop` then spins forever because the sequence
number has moved past what it expects.

The existing C++ tests all used a capacity of 2 or more, so it never
showed up. Both implementations now reject capacities below 2 (a panic in
Rust, `std::invalid_argument` in C++), and both test suites cover it,
including a capacity-2 test that pushes and pops through 1,000 laps.

## Model checking with loom

Ordinary multi-threaded tests only see the interleavings the OS scheduler
happens to produce. loom runs each test many times and systematically
explores the possible interleavings and memory-ordering outcomes of the
atomic operations. The queue is written against `sync.rs`, which switches
between std and loom types at compile time, so the same code is tested
without modification.

To confirm the loom tests actually catch ordering bugs, I weakened the
producer's `Release` store to `Relaxed`. Two of the three loom tests
failed with:

```
Causality violation: Concurrent read and write accesses.
```

which is exactly the bug: a consumer could read the slot before the
producer's write to it was guaranteed to be visible.
