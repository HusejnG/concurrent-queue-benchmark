// producer_consumer.h
//
// Shared producer/consumer logic that works against ANY queue type
// exposing the interface used by both BlockingQueue<T> and LockFreeQueue<T>:
//
//   void push(const T&)
//   bool pop(T&)
//   void shutdown()
//
// This lets the same benchmark code run against the mutex-based queue and
// the lock-free queue without duplicating the threading logic -- only the
// queue implementation underneath changes.

#pragma once

#include <atomic>
#include <chrono>
#include <thread>
#include <vector>

struct Item {
    int producer_id;
    long value;
};

struct RunResult {
    double elapsed_seconds = 0.0;
    long items_produced = 0;
    long items_consumed = 0;
    long long consumed_value_sum = 0;
    long long expected_value_sum = 0;
    bool verified = false;

    double throughput_items_per_sec() const {
        return elapsed_seconds > 0.0 ? static_cast<double>(items_consumed) / elapsed_seconds : 0.0;
    }
};

// Producer i pushes every global index j in [0, n_items) where
// j % n_producers == producer_id. This naturally gives producer 0 any
// remainder items when n_items doesn't divide evenly, and it means the
// full set of values pushed across all producers is exactly the
// permutation {0, 1, ..., n_items - 1} -- which gives us a cheap, exact
// correctness check: the sum of all consumed values must equal
// n_items * (n_items - 1) / 2.
template <typename QueueT>
void producer(QueueT& queue, int producer_id, int n_producers, long n_items) {
    for (long j = producer_id; j < n_items; j += n_producers) {
        queue.push(Item{producer_id, j});
    }
}

template <typename QueueT>
void consumer(QueueT& queue, std::atomic<long>& consumed_count, std::atomic<long long>& consumed_sum) {
    Item item;
    while (queue.pop(item)) {
        consumed_count.fetch_add(1, std::memory_order_relaxed);
        consumed_sum.fetch_add(item.value, std::memory_order_relaxed);
    }
}

// Runs one full producer/consumer cycle against the given queue type and
// returns timing + correctness results. QueueT must be constructible from
// a single std::size_t (buffer capacity).
template <typename QueueT>
RunResult runOnce(long n_items, int n_producers, int n_consumers, std::size_t buffer_size) {
    QueueT queue(buffer_size);
    std::atomic<long> consumed_count{0};
    std::atomic<long long> consumed_sum{0};

    auto start = std::chrono::steady_clock::now();

    std::vector<std::thread> producers;
    producers.reserve(n_producers);
    for (int p = 0; p < n_producers; ++p) {
        producers.emplace_back(producer<QueueT>, std::ref(queue), p, n_producers, n_items);
    }

    std::vector<std::thread> consumers;
    consumers.reserve(n_consumers);
    for (int c = 0; c < n_consumers; ++c) {
        consumers.emplace_back(consumer<QueueT>, std::ref(queue), std::ref(consumed_count), std::ref(consumed_sum));
    }

    for (auto& t : producers) t.join();
    queue.shutdown(); // no more items coming; wake any blocked consumers
    for (auto& t : consumers) t.join();

    auto end = std::chrono::steady_clock::now();

    RunResult result;
    result.elapsed_seconds = std::chrono::duration<double>(end - start).count();
    result.items_produced = n_items;
    result.items_consumed = consumed_count.load();
    result.consumed_value_sum = consumed_sum.load();
    result.expected_value_sum = static_cast<long long>(n_items) * (n_items - 1) / 2;
    result.verified = (result.items_consumed == n_items) &&
                      (result.consumed_value_sum == result.expected_value_sum);
    return result;
}
