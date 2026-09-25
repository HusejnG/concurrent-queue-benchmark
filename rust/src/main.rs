//! CLI benchmark driver, a port of `src/main.cpp`. Accepts the same flags
//! and prints the same report format, so the Rust and C++ results can be
//! compared side by side.
//!
//! Usage:
//!   pc_benchmark --nItems 1000000 --nProducers 4 --nConsumers 4 --bufferSize 1024 --mode both

// Under `--cfg loom` the queue uses loom's instrumented atomics, which only
// work inside `loom::model`, so the benchmark is compiled out and only the
// model-checking tests run.
#[cfg(not(loom))]
mod app {
    use std::process::ExitCode;

    use cq_rust::harness::{run_once, Item, RunResult};
    use cq_rust::{LockFreeQueue, MutexQueue};

    struct Args {
        n_items: u64,
        n_producers: usize,
        n_consumers: usize,
        buffer_size: usize,
        mode: String,
    }

    fn parse_args() -> Args {
        let mut args = Args {
            n_items: 1_000_000,
            n_producers: 4,
            n_consumers: 4,
            buffer_size: 1024,
            mode: "both".to_string(),
        };
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let mut i = 0;
        while i < raw.len() {
            let value = raw.get(i + 1);
            match (raw[i].as_str(), value) {
                ("--nItems", Some(v)) => {
                    args.n_items = v.parse().expect("--nItems expects a number")
                }
                ("--nProducers", Some(v)) => {
                    args.n_producers = v.parse().expect("--nProducers expects a number")
                }
                ("--nConsumers", Some(v)) => {
                    args.n_consumers = v.parse().expect("--nConsumers expects a number")
                }
                ("--bufferSize", Some(v)) => {
                    args.buffer_size = v.parse().expect("--bufferSize expects a number")
                }
                ("--mode", Some(v)) => args.mode = v.clone(),
                ("--help", _) => {
                    println!(
                        "Usage: pc_benchmark --nItems N --nProducers P --nConsumers C \
                         --bufferSize B --mode mutex|lockfree|both"
                    );
                    std::process::exit(0);
                }
                _ => {
                    i += 1;
                    continue;
                }
            }
            i += 2;
        }
        args
    }

    fn print_result(label: &str, r: &RunResult) {
        println!(
            "{:<12}  time: {:.4} s  throughput: {:.0} items/s  consumed: {}  verified: {}",
            label,
            r.elapsed_seconds,
            r.throughput_items_per_sec(),
            r.items_consumed,
            if r.verified { "OK" } else { "MISMATCH" }
        );
    }

    pub fn main() -> ExitCode {
        let args = parse_args();
        println!("Producer/Consumer benchmark (Rust)");
        println!(
            "  nItems={} nProducers={} nConsumers={} bufferSize={} mode={}\n",
            args.n_items, args.n_producers, args.n_consumers, args.buffer_size, args.mode
        );

        let run_mutex = args.mode == "mutex" || args.mode == "both";
        let run_lockfree = args.mode == "lockfree" || args.mode == "both";

        let mutex_result = run_mutex.then(|| {
            run_once::<MutexQueue<Item>>(
                args.n_items,
                args.n_producers,
                args.n_consumers,
                args.buffer_size,
            )
        });
        let lockfree_result = run_lockfree.then(|| {
            run_once::<LockFreeQueue<Item>>(
                args.n_items,
                args.n_producers,
                args.n_consumers,
                args.buffer_size,
            )
        });

        println!("Results:");
        if let Some(r) = &mutex_result {
            print_result("mutex", r);
        }
        if let Some(r) = &lockfree_result {
            print_result("lockfree", r);
        }
        if let (Some(m), Some(l)) = (&mutex_result, &lockfree_result) {
            if l.elapsed_seconds > 0.0 {
                println!(
                    "\nLock-free vs mutex speedup: {:.2}x",
                    m.elapsed_seconds / l.elapsed_seconds
                );
            }
        }

        let all_ok = mutex_result.map_or(true, |r| r.verified)
            && lockfree_result.map_or(true, |r| r.verified);
        if all_ok {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(loom))]
fn main() -> std::process::ExitCode {
    app::main()
}

#[cfg(loom)]
fn main() {}
