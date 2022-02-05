use std::sync::Arc;

use stack::Stack;

const NUM_LOOP: usize = 100_000;
const NUM_THREADS: usize = 4;

fn main() {
    let stack = Arc::new(Stack::<usize>::new());
    let v = (0..NUM_THREADS)
        .map(|i| {
            let stack0 = stack.clone();
            std::thread::spawn(move || {
                if i & 1 == 0 {
                    for j in 0..NUM_LOOP {
                        let k = i * NUM_LOOP + j;
                        stack0.get_mut().push(k);
                        println!("push: {}", k);
                    }
                    println!("finished push: #{}", i);
                } else {
                    for _ in 0..NUM_LOOP {
                        loop {
                            if let Some(k) = stack0.get_mut().pop() {
                                println!("pop: {}", k);
                                break;
                            }
                        }
                    }
                    println!("finished pop: #{}", i);
                }
            })
        })
        .collect::<Vec<_>>();

    for t in v {
        t.join().unwrap();
    }

    assert_eq!(stack.get_mut().pop(), None)
}
