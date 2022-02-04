use std::sync::Arc;

use oreilly_concurrent::fair_lock::FairLock;

const NUM_LOOP: usize = 100_000;
const NUM_THREADS: usize = 4;

fn main() {
    let lock = Arc::new(FairLock::new(0));
    let v: Vec<_> = (0..NUM_THREADS)
        .map(|i| {
            let lock0 = lock.clone();
            std::thread::spawn(move || {
                for _ in 0..NUM_LOOP {
                    let mut data = lock0.lock(i);
                    *data += 1;
                }
            })
        })
        .collect();

    for t in v {
        t.join().unwrap();
    }

    println!(
        "COUNT = {} (expected = {})",
        *lock.lock(0),
        NUM_LOOP * NUM_THREADS
    );
}
