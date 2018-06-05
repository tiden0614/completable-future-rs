extern crate futures;
extern crate completable_future;

use futures::executor::block_on;
use std::thread::spawn;
use std::sync::{Arc, Mutex};
use completable_future::CompletableFuture;

type TestFuture = CompletableFuture<i32, String>;

fn completed(val: i32) -> TestFuture {
	CompletableFuture::completed(val)
}

fn errored(err: &str) -> TestFuture {
	CompletableFuture::errored(err.to_string())
}

fn new() -> TestFuture {
	CompletableFuture::new()
}

#[test]
fn test_completed_future() {
	let fut = completed(0);
	assert_eq!(0, block_on(fut).unwrap());
}

#[test]
fn test_errored_future() {
	let fut = errored("err");
	assert_eq!("err", block_on(fut).unwrap_err());
}

#[test]
fn test_signal_complete() {
	let fut = new();
	let mut signal = fut.signal();

	spawn(move || {
		signal.complete(3);
	});

	assert_eq!(3, block_on(fut).unwrap());
}

#[test]
fn test_signal_error() {
	let fut = new();
	let mut signal = fut.signal();

	spawn(move || {
		signal.error("err".to_string());
	});

	assert_eq!("err", block_on(fut).unwrap_err());
}

#[test]
fn test_complete_once() {
	let mutate_counter = Arc::new(Mutex::new(0));
	let fut = new();
	let signal = fut.signal();

	for i in 0..10 {
		let mut ts = signal.clone();
		let mut tm = mutate_counter.clone();
		spawn(move || {
			if ts.complete(i) {
				let mut mc = tm.lock().unwrap();
				*mc += 1;
			}
		});
	}

	let _res = block_on(fut);
	let mc = mutate_counter.lock().unwrap();
	assert_eq!(*mc, 1);
}

#[test]
fn test_error_once() {
	let mutate_counter = Arc::new(Mutex::new(0));
	let fut = new();
	let signal = fut.signal();

	for i in 0..10 {
		let mut ts = signal.clone();
		let mut tm = mutate_counter.clone();
		spawn(move || {
			if ts.error(format!("{}", i)) {
				let mut mc = tm.lock().unwrap();
				*mc += 1;
			}
		});
	}

	let _res = block_on(fut);
	let mc = mutate_counter.lock().unwrap();
	assert_eq!(*mc, 1);
}