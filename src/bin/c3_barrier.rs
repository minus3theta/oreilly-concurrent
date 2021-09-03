use std::{
    sync::{Arc, Barrier},
    thread,
};

fn main() {
    let mut v = Vec::new();

    let barrier = Arc::new(Barrier::new(10));

    for _ in 0..10 {
        let b = barrier.clone();
        let th = thread::spawn(move || {
            b.wait();
            println!("finished barrier");
        });
        v.push(th);
    }

    for th in v {
        th.join().unwrap();
    }
}
