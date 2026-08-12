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

Unit tests are built automatically (Google Test is fetched via
`FetchContent`, no manual install needed).

## Running the benchmark

```bash
./build/pc_benchmark --nItems 2000000 --nProducers 4 --nConsumers 4 --bufferSize 1024 --mode both
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

Measured on a **1-vCPU cloud sandbox** (Intel Xeon @ 2.80GHz) — a
single-core machine means producers and consumers are time-sliced rather
than truly running in parallel, so these numbers understate the advantage
a real multi-core machine would show. Re-run `pc_benchmark` on your own
hardware (`nproc` to check core count) for representative numbers before
quoting them anywhere serious.

`--nItems 2000000`, buffer size 1024:

| Producers × Consumers | Mutex (items/s) | Lock-free (items/s) | Speedup |
|---|---|---|---|
| 1 × 1 | 7,273,821 | 21,406,181 | 2.94x |
| 4 × 4 | 6,475,409 | 19,551,197 | 3.02x |
| 8 × 8 | 3,813,987 | 17,708,755 | 4.64x |

With a deliberately tiny buffer (size 16, `4 × 4` threads) to force
constant blocking/contention:

| Queue | Throughput (items/s) | Speedup |
|---|---|---|
| Mutex | 723,745 | — |
| Lock-free | 2,701,093 | 3.73x |

The pattern holds across every configuration tested here: the mutex-based
queue's throughput degrades faster as thread count grows or buffer size
shrinks, because more threads means more contention on the same lock and
more OS-level wake-ups. The lock-free queue's CAS loop degrades more
gracefully under the same pressure.

## Possible extensions

- Latency histograms (p50/p99) in addition to aggregate throughput
- A single-producer/single-consumer lock-free variant (can drop the CAS
  loop entirely and just use plain atomic loads/stores)
- NUMA-aware benchmarking on multi-socket machines
