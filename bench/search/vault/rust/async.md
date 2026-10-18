# Async

Rust async functions return futures that an executor polls. Await suspends the
task without blocking a thread. An async runtime schedules many tasks over few
threads.
