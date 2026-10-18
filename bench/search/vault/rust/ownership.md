# Ownership

Rust ownership is the core memory safety model. Each value has one owner, and
the borrow checker enforces references so that data races and use-after-free are
impossible at compile time.
