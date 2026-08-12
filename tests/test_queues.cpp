// test_queues.cpp
//
// Correctness tests for BlockingQueue and LockFreeQueue, run through the
// shared producer/consumer harness. The key invariant under test: with
// n_items spread across n_producers using the round-robin scheme in
// producer_consumer.h, every value in [0, n_items) must be produced and
// consumed exactly once, regardless of how many producer/consumer threads
// are racing over the queue.

#include <gtest/gtest.h>

#include "blocking_queue.h"
#include "lockfree_queue.h"
#include "producer_consumer.h"

namespace {

void expect_no_lost_items(const RunResult& r, long n_items) {
    EXPECT_EQ(r.items_consumed, n_items) << "consumer count does not match items produced";
    EXPECT_EQ(r.consumed_value_sum, r.expected_value_sum)
        << "sum of consumed values does not match expected sum -- an item was "
           "lost, duplicated, or corrupted";
    EXPECT_TRUE(r.verified);
}

} // namespace

// ---- BlockingQueue --------------------------------------------------------

TEST(BlockingQueue, SingleProducerSingleConsumer) {
    auto r = runOnce<BlockingQueue<Item>>(/*n_items=*/10'000, /*producers=*/1, /*consumers=*/1, /*buffer=*/16);
    expect_no_lost_items(r, 10'000);
}

TEST(BlockingQueue, MultiProducerMultiConsumer) {
    auto r = runOnce<BlockingQueue<Item>>(/*n_items=*/200'000, /*producers=*/4, /*consumers=*/4, /*buffer=*/64);
    expect_no_lost_items(r, 200'000);
}

TEST(BlockingQueue, TinyBufferForcesBlocking) {
    // A buffer of size 1 forces producers and consumers to constantly block
    // on each other -- a good stress test of the wait/notify logic.
    auto r = runOnce<BlockingQueue<Item>>(/*n_items=*/5'000, /*producers=*/3, /*consumers=*/3, /*buffer=*/1);
    expect_no_lost_items(r, 5'000);
}

TEST(BlockingQueue, ItemCountNotEvenlyDivisible) {
    // n_items not divisible by n_producers exercises the "remainder goes to
    // producer 0" path.
    auto r = runOnce<BlockingQueue<Item>>(/*n_items=*/10'007, /*producers=*/3, /*consumers=*/2, /*buffer=*/32);
    expect_no_lost_items(r, 10'007);
}

// ---- LockFreeQueue --------------------------------------------------------

TEST(LockFreeQueue, SingleProducerSingleConsumer) {
    auto r = runOnce<LockFreeQueue<Item>>(/*n_items=*/10'000, /*producers=*/1, /*consumers=*/1, /*buffer=*/16);
    expect_no_lost_items(r, 10'000);
}

TEST(LockFreeQueue, MultiProducerMultiConsumer) {
    auto r = runOnce<LockFreeQueue<Item>>(/*n_items=*/200'000, /*producers=*/4, /*consumers=*/4, /*buffer=*/64);
    expect_no_lost_items(r, 200'000);
}

TEST(LockFreeQueue, TinyBufferForcesContention) {
    auto r = runOnce<LockFreeQueue<Item>>(/*n_items=*/5'000, /*producers=*/3, /*consumers=*/3, /*buffer=*/2);
    expect_no_lost_items(r, 5'000);
}

TEST(LockFreeQueue, ItemCountNotEvenlyDivisible) {
    auto r = runOnce<LockFreeQueue<Item>>(/*n_items=*/10'007, /*producers=*/3, /*consumers=*/2, /*buffer=*/32);
    expect_no_lost_items(r, 10'007);
}

// ---- Direct queue API tests (no threads) ----------------------------------

TEST(LockFreeQueueApi, TryPushFailsWhenFull) {
    LockFreeQueue<int> q(2);
    EXPECT_TRUE(q.try_push(1));
    EXPECT_TRUE(q.try_push(2));
    EXPECT_FALSE(q.try_push(3)); // capacity 2, already full
}

TEST(LockFreeQueueApi, TryPopFailsWhenEmpty) {
    LockFreeQueue<int> q(2);
    int out;
    EXPECT_FALSE(q.try_pop(out));
    EXPECT_TRUE(q.try_push(42));
    EXPECT_TRUE(q.try_pop(out));
    EXPECT_EQ(out, 42);
    EXPECT_FALSE(q.try_pop(out));
}
