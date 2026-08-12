// main.cpp
//
// CLI driver for the producer/consumer benchmark. Runs the same workload
// against the mutex-based BlockingQueue and the atomics-based
// LockFreeQueue and prints a side-by-side comparison.
//
// Usage:
//   ./pc_benchmark --nItems 1000000 --nProducers 4 --nConsumers 4 --bufferSize 1024 --mode both
//
// --mode accepts: mutex | lockfree | both   (default: both)

#include "blocking_queue.h"
#include "lockfree_queue.h"
#include "producer_consumer.h"

#include <cstring>
#include <iomanip>
#include <iostream>
#include <string>

namespace {

struct Args {
    long n_items = 1'000'000;
    int n_producers = 4;
    int n_consumers = 4;
    std::size_t buffer_size = 1024;
    std::string mode = "both";
};

long parse_long(const char* s) { return std::strtol(s, nullptr, 10); }

Args parse_args(int argc, char** argv) {
    Args args;
    for (int i = 1; i < argc; ++i) {
        std::string key = argv[i];
        if (key == "--nItems" && i + 1 < argc) {
            args.n_items = parse_long(argv[++i]);
        } else if (key == "--nProducers" && i + 1 < argc) {
            args.n_producers = static_cast<int>(parse_long(argv[++i]));
        } else if (key == "--nConsumers" && i + 1 < argc) {
            args.n_consumers = static_cast<int>(parse_long(argv[++i]));
        } else if (key == "--bufferSize" && i + 1 < argc) {
            args.buffer_size = static_cast<std::size_t>(parse_long(argv[++i]));
        } else if (key == "--mode" && i + 1 < argc) {
            args.mode = argv[++i];
        } else if (key == "--help") {
            std::cout << "Usage: pc_benchmark --nItems N --nProducers P --nConsumers C "
                         "--bufferSize B --mode mutex|lockfree|both\n";
            std::exit(0);
        }
    }
    return args;
}

void printResult(const std::string& label, const RunResult& r) {
    std::cout << std::left << std::setw(12) << label
              << "  time: " << std::fixed << std::setprecision(4) << r.elapsed_seconds << " s"
              << "  throughput: " << std::setprecision(0) << r.throughput_items_per_sec() << " items/s"
              << "  consumed: " << r.items_consumed
              << "  verified: " << (r.verified ? "OK" : "MISMATCH")
              << "\n";
}

} // namespace

int main(int argc, char** argv) {
    Args args = parse_args(argc, argv);

    std::cout << "Producer/Consumer benchmark\n"
              << "  nItems=" << args.n_items
              << " nProducers=" << args.n_producers
              << " nConsumers=" << args.n_consumers
              << " bufferSize=" << args.buffer_size
              << " mode=" << args.mode << "\n\n";

    RunResult mutex_result, lockfree_result;
    bool ran_mutex = false, ran_lockfree = false;

    if (args.mode == "mutex" || args.mode == "both") {
        mutex_result = runOnce<BlockingQueue<Item>>(args.n_items, args.n_producers, args.n_consumers, args.buffer_size);
        ran_mutex = true;
    }
    if (args.mode == "lockfree" || args.mode == "both") {
        lockfree_result = runOnce<LockFreeQueue<Item>>(args.n_items, args.n_producers, args.n_consumers, args.buffer_size);
        ran_lockfree = true;
    }

    std::cout << "Results:\n";
    if (ran_mutex) printResult("mutex", mutex_result);
    if (ran_lockfree) printResult("lockfree", lockfree_result);

    if (ran_mutex && ran_lockfree && mutex_result.elapsed_seconds > 0.0) {
        double speedup = mutex_result.elapsed_seconds / lockfree_result.elapsed_seconds;
        std::cout << "\nLock-free vs mutex speedup: " << std::fixed << std::setprecision(2)
                  << speedup << "x\n";
    }

    bool all_ok = (!ran_mutex || mutex_result.verified) && (!ran_lockfree || lockfree_result.verified);
    return all_ok ? 0 : 1;
}
