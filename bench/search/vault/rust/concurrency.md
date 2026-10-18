# Concurrency

Rust concurrency is built on threads that send messages over channels. A mutex
guards shared state, and the Send and Sync traits declare which types may cross
thread boundaries.
