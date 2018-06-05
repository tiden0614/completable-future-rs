# Completable Future

Similar to Java's CompletableFuture, this crate provides a simple
future that can be completed and properly notified from elsewhere other
than the executor of the future. It is sutable for some blocking
tasks that could block the executor if we use a future directly in
an executor.

A CompletableFuture is still a future and has all the combinators that
you can use to chain logic working on the result or the error. Also,
unlike Java and inherited from Rust's poll model future, some executor
needs to execute the CompletableFuture in order to get the result; the
thread or code that completes (or errors) the future will not execute
the logic chained after the future.

The CompletableFuture uses Arc and Mutex to synchronize poll and completion,
so there's overhead for using it.

# Example
```
extern crate futures;
extern crate tokio;
extern crate completable_future;

use futures::prelude::*;
use tokio::executor::current_thread::block_on_all;
use std::thread::spawn;
use std::thread::sleep;
use std::time::Duration;

let fut1 = CompletableFuture::<String, ()>::new();
// we will give the signal to some worker for it to complete
let mut signal = fut1.signal(); 
let fut2 = fut1.and_then(|s| {
    // this will come from whoever completes the future
    println!("in fut2: {}", s);
    Ok("this comes from fut2".to_string())
});

let j = spawn(move || {
    println!("waiter thread: I'm going to block on fut2");
    let ret = block_on_all(fut2).unwrap();
    println!("waiter thread: fut2 completed with message -- {}", ret);
});

spawn(move || {
    println!("worker thread: going to block for 1000 ms");
    sleep(Duration::from_millis(1000));
    signal.complete("this comes from fut1".to_string());
    println!("worker thread: completed fut1");
});

j.join().unwrap();
```