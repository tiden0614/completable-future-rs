//! # Completable Future
//!
//! Similar to Java's CompletableFuture, this crate provides a simple
//! future that can be completed and properly notified from elsewhere other
//! than the executor of the future. It is sutable for some blocking
//! tasks that could block the executor if we use a future directly in
//! an executor.
//!
//! A CompletableFuture is still a future and has all the combinators that
//! you can use to chain logic working on the result or the error. Also,
//! unlike Java and inherited from Rust's poll model future, some executor
//! needs to execute the CompletableFuture in order to get the result; the
//! thread or code that completes (or errors) the future will not execute
//! the logic chained after the future.
//!
//! The CompletableFuture uses Arc and Mutex to synchronize poll and completion,
//! so there's overhead for using it.
//!
//! # Example
//! ```
//! extern crate futures;
//! extern crate completable_future;
//!
//! use futures::{prelude::*, executor::block_on};
//! use std::{
//!     thread::{spawn, sleep},
//!     time::Duration,
//! };
//! use completable_future::CompletableFuture;
//!
//! fn main() {
//!     let fut1: CompletableFuture<String, ()> = CompletableFuture::new();
//!     // we will give the signal to some worker for it to complete
//!     let mut signal = fut1.signal();
//!     let fut2 = fut1.and_then(|s| async move {
//!         // this will come from whoever completes the future
//!         println!("in fut2: {}", s);
//!         Ok("this comes from fut2".to_string())
//!     });
//!     
//!     let j = spawn(move || {
//!         println!("waiter thread: I'm going to block on fut2");
//!         let ret = block_on(fut2).unwrap();
//!         println!("waiter thread: fut2 completed with message -- {}", ret);
//!     });
//!     
//!     spawn(move || {
//!         println!("worker thread: going to block for 1000 ms");
//!         sleep(Duration::from_millis(1000));
//!         signal.complete("this comes from fut1".to_string());
//!         println!("worker thread: completed fut1");
//!     });
//!     
//!     j.join().unwrap();
//! }
//! ```

extern crate futures;

use futures::{
    future::Future,
    task::{AtomicWaker, Context, Poll, Waker},
};
use std::{
    mem,
    pin::Pin,
    sync::{Arc, Mutex},
};

enum WakerWrapper {
    Registered(AtomicWaker),
    NotRegistered,
}

impl WakerWrapper {
    fn register(&mut self, waker: &Waker) {
        match self {
            &mut WakerWrapper::Registered(ref _dont_care) => (),
            &mut WakerWrapper::NotRegistered => {
                let w = AtomicWaker::new();
                w.register(waker);
                let _old = mem::replace(self, WakerWrapper::Registered(w));
            }
        }
    }

    fn wake(&self) {
        match self {
            &WakerWrapper::Registered(ref w) => w.wake(),
            &WakerWrapper::NotRegistered => (),
        };
    }
}

enum FutureState<V, E> {
    Pending,
    Completed(V),
    Errored(E),
    Taken,
}

impl<V, E> FutureState<V, E> {
    fn swap(&mut self, new_val: FutureState<V, E>) -> FutureState<V, E> {
        mem::replace(self, new_val)
    }

    fn unwrap_val(&mut self) -> V {
        match self.swap(FutureState::Taken) {
            FutureState::Completed(val) => val,
            _ => panic!("cannot unwrap because my state is not completed"),
        }
    }

    fn unwrap_err(&mut self) -> E {
        match self.swap(FutureState::Taken) {
            FutureState::Errored(val) => val,
            _ => panic!("cannot unwrap because my state is not errored"),
        }
    }
}

/// the state of the future; reference counted
struct SignalInternal<V, E> {
    waker: WakerWrapper,
    state: FutureState<V, E>,
}

/// A handle to the future state. When you create a completable future,
/// you should also create a signal that somebody can use to complete
/// the future.
#[derive(Clone)]
pub struct CompletableFutureSignal<V, E> {
    internal: Arc<Mutex<SignalInternal<V, E>>>,
}

impl<V, E> CompletableFutureSignal<V, E> {
    fn mutate_self(&mut self, new_state: FutureState<V, E>) -> bool {
        let mut internal = self.internal.lock().unwrap();
        match internal.state {
            FutureState::Pending => {
                internal.state.swap(new_state);
                internal.waker.wake();
                true
            }
            _ => false,
        }
    }

    /// Complete the associated CompletableFuture. This method
    /// can be called safely across multiple threads multiple times,
    /// but only the winning call would mutate the future; other calls
    /// will be rendered noop.
    ///
    /// Returns whether the call successfully mutates the future.
    pub fn complete(&mut self, value: V) -> bool {
        self.mutate_self(FutureState::Completed(value))
    }

    /// Error the associated CompletableFuture. This method
    /// can be called safely across multiple threads multiple times,
    /// but only the winning call would mutate the future; other calls
    /// will be rendered noop.
    ///
    /// Returns whether the call successfully mutates the future.
    pub fn error(&mut self, error: E) -> bool {
        self.mutate_self(FutureState::Errored(error))
    }
}

/// A CompletableFuture is a future that you can expect a result (or error)
/// from and chain logic on. You will need some executor to actively poll
/// the result. Executors provided by the futures crate are usually good
/// enough for common situations.
///
/// If you use a custom executor, be careful that don't poll the CompletableFuture
/// after it has already completed (or errored) in previous polls. Doing so
/// will panic your executor.
pub struct CompletableFuture<V, E> {
    internal: Arc<Mutex<SignalInternal<V, E>>>,
}

impl<V, E> CompletableFuture<V, E> {
    /// Construct a CompletableFuture.
    pub fn new() -> CompletableFuture<V, E> {
        CompletableFuture {
            internal: Arc::new(Mutex::new(SignalInternal {
                waker: WakerWrapper::NotRegistered,
                state: FutureState::Pending,
            })),
        }
    }

    /// Construct a CompletableFuture that's already completed
    /// with the value provided.
    pub fn completed(val: V) -> CompletableFuture<V, E> {
        CompletableFuture {
            internal: Arc::new(Mutex::new(SignalInternal {
                waker: WakerWrapper::NotRegistered,
                state: FutureState::Completed(val),
            })),
        }
    }

    /// Construct a CompletableFuture that's already errored
    /// with the value provided.
    pub fn errored(e: E) -> CompletableFuture<V, E> {
        CompletableFuture {
            internal: Arc::new(Mutex::new(SignalInternal {
                waker: WakerWrapper::NotRegistered,
                state: FutureState::Errored(e),
            })),
        }
    }

    /// Get a CompletableFutureSignal that can be used to complete
    /// or error this CompletableFuture.
    pub fn signal(&self) -> CompletableFutureSignal<V, E> {
        CompletableFutureSignal {
            internal: self.internal.clone(),
        }
    }
}

impl<V, E> Future for CompletableFuture<V, E> {
    type Output = Result<V, E>;

    fn poll(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<V, E>> {
        let mut signal = self.internal.lock().unwrap();
        signal.waker.register(ctx.waker());

        let state = &mut signal.state;
        match state {
            &mut FutureState::Pending => Poll::Pending,
            &mut FutureState::Taken => {
                panic!("bug: the value has been taken, yet I'm still polled again")
            }
            &mut FutureState::Completed(_) => Poll::Ready(Ok(state.unwrap_val())),
            &mut FutureState::Errored(_) => Poll::Ready(Err(state.unwrap_err())),
        }
    }
}
