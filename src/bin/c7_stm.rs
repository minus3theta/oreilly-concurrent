use core::time;
use std::sync::Arc;

use oreilly_concurrent::tl2;

#[macro_export]
macro_rules! load {
    ($t:ident, $a:expr) => {
        if let Some(v) = ($t).load($a) {
            v
        } else {
            return tl2::STMResult::Retry;
        }
    };
}

#[macro_export]
macro_rules! store {
    ($t:ident, $a:expr, $v:expr) => {
        $t.store($a, $v)
    };
}

const NUM_PHILOSOPHERS: usize = 8;

fn philosopher(stm: Arc<tl2::STM>, n: usize) {
    let left = 8 * n;
    let right = 8 * ((n + 1) % NUM_PHILOSOPHERS);

    for _ in 0..500_000 {
        while !stm
            .write_tansaction(|tr| {
                let mut f1 = load!(tr, left);
                let mut f2 = load!(tr, right);
                if f1[0] == 0 && f2[0] == 0 {
                    f1[0] = 1;
                    f2[0] = 1;
                    store!(tr, left, f1);
                    store!(tr, right, f2);
                    tl2::STMResult::Ok(true)
                } else {
                    tl2::STMResult::Ok(false)
                }
            })
            .unwrap()
        {}

        stm.write_tansaction(|tr| {
            let mut f1 = load!(tr, left);
            let mut f2 = load!(tr, right);
            f1[0] = 0;
            f2[0] = 0;
            store!(tr, left, f1);
            store!(tr, right, f2);
            tl2::STMResult::Ok(())
        });
    }
}

fn observer(stm: Arc<tl2::STM>) {
    for _ in 0..10000 {
        let chopsticks = stm
            .read_transaction(|tr| {
                let mut v = [0; NUM_PHILOSOPHERS];
                for i in 0..NUM_PHILOSOPHERS {
                    v[i] = load!(tr, 8 * i)[0];
                }

                tl2::STMResult::Ok(v)
            })
            .unwrap();

        println!("{:?}", chopsticks);

        let mut n = 0;
        for c in &chopsticks {
            if *c == 1 {
                n += 1;
            }
        }

        if n & 1 != 0 {
            panic!("inconsistent");
        }

        let us = time::Duration::from_micros(100);
        std::thread::sleep(us);
    }
}

fn main() {
    let stm = Arc::new(tl2::STM::new());
    let v: Vec<_> = (0..NUM_PHILOSOPHERS)
        .map(|i| {
            let s = stm.clone();
            std::thread::spawn(move || philosopher(s, i))
        })
        .collect();

    let obs = std::thread::spawn(move || observer(stm));

    for th in v {
        th.join().unwrap();
    }

    obs.join().unwrap();
}
