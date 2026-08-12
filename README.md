# Concurrent Queue Benchmark

A multi-producer/multi-consumer benchmark in modern C++ comparing two
thread-safe bounded queue implementations:

- **`BlockingQueue<T>`** — a classic design built on `std::mutex` and
  `std::condition_variable`. Producers block when the queue is full,
  consumers block when it's empty.
- **`LockFreeQueue<T>`** — a bounded MPMC ring buffer built on
  `std::atomic`, following the Dmitry Vyukov sequence-number design. No
  mutex, no OS-level blocking — just compare-and-swap loops.

Both queues expose the same interface (`push`, `pop`, `shutdown`), so the
exact same producer/consumer driver code (`include/producer_consumer.h`)
runs against either one. This lets the benchmark isolate the effect of the
synchronization strategy itself, rather than comparing two different
programs.

## Why compare these two designs

A mutex is simple and correct, but it interacts with the OS scheduler: a
thread can be preempted while holding the lock, and waking a blocked thread
means a context switch. That's fine for most software, but it makes
worst-case latency hard to bound — which matters in real-time and embedded
contexts (e.g. handing data from an interrupt handler to a main loop, where
you cannot afford to have that handoff blocked on scheduling).

A lock-free queue never blocks a thread on another thread's scheduling
state. The trade-off is that a busy producer/consumer will spin (yielding
the CPU) instead of sleeping, and the implementation itself is
considerably harder to get right (memory ordering, ABA-style hazards,
sequence numbers per slot).

## Project layout

```
concurrent-queue-benchmark/
├── include/
│   ├── blocking_queue.h       # mutex + condition_variable queue
│   ├── lockfree_queue.h       # atomics-based MPMC ring buffer
│   └── producer_consumer.h    # shared producer/consumer + benchmark harness
├── src/
│   └── main.cpp                # CLI driver, runs both queues, prints comparison
├── tests/
│   └── test_queues.cpp         # Google Test correctness tests
├── .github/workflows/ci.yml    # build + test on push (Linux & Windows)
└── CMakeLists.txt
```

## Building

Requires a C++17 compiler and CMake 3.16+. Works identically on Linux,
macOS, and Windows (MSVC or MinGW) — there is no OS-specific code path.

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release -j
```

On Windows with Visual Studio, the same two commands work from a Developer
Command Prompt, or you can open the folder directly in Visual Studio /
VS Code with the CMake Tools extension.

**Important:** always benchmark a **Release** build. A Debug build (MSVC in
particular) adds significant overhead to STL containers like `std::queue`
via iterator checking, which distorts the mutex-based queue's numbers far
more than the lock-free one and makes the comparison meaningless.

Unit tests are built automatically (Google Test is fetched via
`FetchContent`, no manual install needed).

## Running the benchmark

```bash
./build/Release/pc_benchmark.exe --nItems 2000000 --nProducers 4 --nConsumers 4 --bufferSize 1024 --mode both
```

| Flag | Meaning | Default |
|---|---|---|
| `--nItems` | total items produced across all producers | 1000000 |
| `--nProducers` | number of producer threads | 4 |
| `--nConsumers` | number of consumer threads | 4 |
| `--bufferSize` | bounded queue capacity | 1024 |
| `--mode` | `mutex`, `lockfree`, or `both` | both |

Correctness is verified on every run: producer *i* generates every global
index `j` where `j % nProducers == i`, so the full set of values produced
is exactly the permutation `{0, ..., nItems-1}`. The benchmark sums every
consumed value and checks it against `nItems * (nItems-1) / 2` — if a
single item were ever lost, duplicated, or corrupted by a race, this sum
would not match.

## Running the tests

```bash
cd build
ctest --output-on-failure
```

Tests cover both queues under single-threaded, multi-threaded, and
tiny-buffer (high contention) conditions, plus a non-divisible item count
to exercise the remainder-handling path.

## Sample results

Measured on a laptop with an **Intel Core i7-6700HQ @ 2.60GHz (4 cores /
8 logical processors)**, Release build, `--nItems 2000000`, buffer size 1024:

| Producers × Consumers | Mutex (items/s) | Lock-free (items/s) | Speedup |
|---|---|---|---|
| 1 × 1 | 6,745,472 | 34,431,408 | 5.10x |
| 4 × 4 | 3,874,244 | 8,273,365 | 2.14x |
| 8 × 8 | 4,121,476 | 6,287,914 | 1.53x |

Correctness verified on every run (`consumed == nItems`, checksum matches).

The pattern worth noting: **speedup is highest at 1×1 and shrinks as
thread count grows**, which makes sense on a 4-core/8-thread CPU — at 1×1
there's no contention at all, so the benchmark measures pure per-operation
overhead (mutex/condition_variable machinery vs. a handful of atomic
instructions). At 8×8, there are 16 software threads competing for 8
hardware threads, so both queues are increasingly bottlenecked by CPU
availability rather than by the synchronization primitive itself — the
lock-free queue still wins, but the gap narrows because the hardware,
not the queue design, is now the limiting factor.

## Possible extensions

- Latency histograms (p50/p99) in addition to aggregate throughput
- A single-producer/single-consumer lock-free variant (can drop the CAS
  loop entirely and just use plain atomic loads/stores)
- Re-run at thread counts between 1 and physical core count to find where
  contention actually starts to dominate on a given machine
