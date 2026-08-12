// lockfree_queue.h
//
// A bounded, lock-free multi-producer/multi-consumer queue based on the
// classic Dmitry Vyukov ring-buffer design. Each slot carries its own
// sequence number, which lets producers and consumers race over the same
// buffer using only compare-and-swap and atomic loads/stores -- no mutex,
// no condition variable, no blocking syscalls.
//
// Why this matters in an embedded/real-time context: a mutex can be
// preempted while held, which makes worst-case latency unbounded (priority
// inversion). A lock-free structure like this one never blocks a thread on
// another thread's scheduling, which is why designs like this show up in
// interrupt-to-main-loop handoffs and other hard real-time paths.
//
// This implementation exposes both a non-blocking API (try_push/try_pop)
// and a convenience busy-wait API (push/pop) so it can be swapped in for
// BlockingQueue in the benchmark driver without changing calling code.

#pragma once

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <thread>
#include <vector>

template <typename T>
class LockFreeQueue {
public:
    explicit LockFreeQueue(std::size_t capacity)
        : buffer_(capacity), capacity_(capacity) {
        for (std::size_t i = 0; i < capacity_; ++i) {
            buffer_[i].sequence.store(i, std::memory_order_relaxed);
        }
        enqueue_pos_.store(0, std::memory_order_relaxed);
        dequeue_pos_.store(0, std::memory_order_relaxed);
    }

    // Non-blocking. Returns false immediately if the queue is full.
    bool try_push(const T& item) {
        Cell* cell;
        std::size_t pos = enqueue_pos_.load(std::memory_order_relaxed);
        for (;;) {
            cell = &buffer_[pos % capacity_];
            std::size_t seq = cell->sequence.load(std::memory_order_acquire);
            std::intptr_t diff = static_cast<std::intptr_t>(seq) - static_cast<std::intptr_t>(pos);
            if (diff == 0) {
                if (enqueue_pos_.compare_exchange_weak(pos, pos + 1, std::memory_order_relaxed)) {
                    break;
                }
            } else if (diff < 0) {
                return false; // queue full
            } else {
                pos = enqueue_pos_.load(std::memory_order_relaxed);
            }
        }
        cell->data = item;
        cell->sequence.store(pos + 1, std::memory_order_release);
        return true;
    }

    // Non-blocking. Returns false immediately if the queue is empty.
    bool try_pop(T& item) {
        Cell* cell;
        std::size_t pos = dequeue_pos_.load(std::memory_order_relaxed);
        for (;;) {
            cell = &buffer_[pos % capacity_];
            std::size_t seq = cell->sequence.load(std::memory_order_acquire);
            std::intptr_t diff = static_cast<std::intptr_t>(seq) - static_cast<std::intptr_t>(pos + 1);
            if (diff == 0) {
                if (dequeue_pos_.compare_exchange_weak(pos, pos + 1, std::memory_order_relaxed)) {
                    break;
                }
            } else if (diff < 0) {
                return false; // queue empty
            } else {
                pos = dequeue_pos_.load(std::memory_order_relaxed);
            }
        }
        item = cell->data;
        cell->sequence.store(pos + capacity_, std::memory_order_release);
        return true;
    }

    // Busy-wait convenience wrapper so this class can be used as a
    // drop-in replacement for BlockingQueue's push() in the benchmark driver.
    void push(const T& item) {
        while (!try_push(item)) {
            std::this_thread::yield();
        }
    }

    // Busy-wait convenience wrapper matching BlockingQueue::pop()'s contract:
    // returns false once shutdown() has been called and the queue is empty.
    bool pop(T& item) {
        for (;;) {
            if (try_pop(item)) {
                return true;
            }
            if (done_.load(std::memory_order_acquire)) {
                // One more attempt in case an item landed between the
                // failed try_pop above and the done_ check.
                return try_pop(item);
            }
            std::this_thread::yield();
        }
    }

    void shutdown() {
        done_.store(true, std::memory_order_release);
    }

    std::size_t capacity() const { return capacity_; }

private:
    struct Cell {
        std::atomic<std::size_t> sequence;
        T data;
    };

    std::vector<Cell> buffer_;
    std::size_t capacity_;

    // Padded to separate cache lines so producer and consumer indices
    // don't false-share a cache line under contention.
    alignas(64) std::atomic<std::size_t> enqueue_pos_;
    alignas(64) std::atomic<std::size_t> dequeue_pos_;
    alignas(64) std::atomic<bool> done_{false};
};
