// blocking_queue.h
//
// A bounded, thread-safe multi-producer/multi-consumer queue built on
// std::mutex and std::condition_variable. Producers block while the queue
// is full; consumers block while the queue is empty. Call shutdown() once
// all producers have finished so that blocked consumers can wake up and
// drain the remaining items instead of waiting forever.

#pragma once

#include <condition_variable>
#include <mutex>
#include <queue>
#include <cstddef>
#include <utility>

template <typename T>
class BlockingQueue {
public:
    explicit BlockingQueue(std::size_t capacity) : capacity_(capacity) {}

    // Blocks while the queue is full. Safe to call from multiple producer
    // threads concurrently.
    void push(T item) {
        std::unique_lock<std::mutex> lock(mutex_);
        not_full_.wait(lock, [this] { return queue_.size() < capacity_ || done_; });
        queue_.push(std::move(item));
        lock.unlock();
        not_empty_.notify_one();
    }

    // Blocks while the queue is empty and no shutdown has been signalled.
    // Returns false once the queue is empty AND shutdown() has been called,
    // signalling to the caller that no more items will ever arrive.
    bool pop(T& out) {
        std::unique_lock<std::mutex> lock(mutex_);
        not_empty_.wait(lock, [this] { return !queue_.empty() || done_; });
        if (queue_.empty()) {
            return false;
        }
        out = std::move(queue_.front());
        queue_.pop();
        lock.unlock();
        not_full_.notify_one();
        return true;
    }

    // Signals that no more items will be pushed. Wakes up any thread
    // currently blocked in push()/pop() so the program can terminate
    // cleanly once the queue has been drained.
    void shutdown() {
        {
            std::lock_guard<std::mutex> lock(mutex_);
            done_ = true;
        }
        not_empty_.notify_all();
        not_full_.notify_all();
    }

    std::size_t capacity() const { return capacity_; }

private:
    mutable std::mutex mutex_;
    std::condition_variable not_full_;
    std::condition_variable not_empty_;
    std::queue<T> queue_;
    std::size_t capacity_;
    bool done_ = false;
};
