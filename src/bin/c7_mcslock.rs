use std::sync::Arc;

use oreilly_concurrent::mcs_lock::{MCSLock, MCSNode};

const NUM_LOOP: usize = 100_000;
const NUM_THREADS: usize = 4;

fn main() {
    let n = Arc::new(MCSLock::new(0));
    let v: Vec<_> = (0..NUM_THREADS)
        .map(|_| {
            let n0 = n.clone();
            std::thread::spawn(move || {
                let mut node = MCSNode::new();
                for _ in 0..NUM_LOOP {
                    let mut r = n0.lock(&mut node);
                    *r += 1;
                }
            })
        })
        .collect();

    for t in v {
        t.join().unwrap();
    }

    let mut node = MCSNode::new();
    let r = n.lock(&mut node);
    println!("COUNT = {} (expected = {})", *r, NUM_LOOP * NUM_THREADS);
}
