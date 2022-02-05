use std::{
    cell::UnsafeCell,
    collections::{HashMap, HashSet},
    convert::TryInto,
    sync::atomic::{fence, AtomicU64, Ordering},
};

const STRIPE_SIZE: usize = 8;
const MEM_SIZE: usize = 512;
const STRIPE_NUM: usize = MEM_SIZE / STRIPE_SIZE;

pub struct Memory {
    mem: [u8; MEM_SIZE],
    lock_ver: [AtomicU64; STRIPE_NUM],
    global_clock: AtomicU64,
}

impl Memory {
    const SHIFT_SIZE: u32 = STRIPE_SIZE.trailing_zeros();

    pub fn new() -> Self {
        let mem = [0; MEM_SIZE];

        let lock_ver = (0..MEM_SIZE >> Self::SHIFT_SIZE)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        Self {
            mem,
            lock_ver,
            global_clock: AtomicU64::new(0),
        }
    }

    fn inc_global_clock(&mut self) -> u64 {
        self.global_clock.fetch_add(1, Ordering::AcqRel)
    }

    fn get_addr_ver(&self, addr: usize) -> u64 {
        let idx = addr >> Self::SHIFT_SIZE;
        let n = self.lock_ver[idx].load(Ordering::Relaxed);
        n & !(1 << 63)
    }

    fn test_not_modify(&self, addr: usize, rv: u64) -> bool {
        let idx = addr >> Self::SHIFT_SIZE;
        let n = self.lock_ver[idx].load(Ordering::Relaxed);
        n <= rv
    }

    fn lock_addr(&mut self, addr: usize) -> bool {
        let idx = addr >> Self::SHIFT_SIZE;
        self.lock_ver[idx]
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |val| {
                let n = val & (1 << 63);
                if n == 0 {
                    Some(val | (1 << 63))
                } else {
                    None
                }
            })
            .is_ok()
    }

    fn unlock_addr(&mut self, addr: usize) {
        let idx = addr >> Self::SHIFT_SIZE;
        self.lock_ver[idx].fetch_and(!(1 << 63), Ordering::Relaxed);
    }
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ReadTrans<'a> {
    read_ver: u64,
    is_abort: bool,
    mem: &'a Memory,
}

impl<'a> ReadTrans<'a> {
    fn new(mem: &'a Memory) -> Self {
        Self {
            read_ver: mem.global_clock.load(Ordering::Acquire),
            is_abort: false,
            mem,
        }
    }

    pub fn load(&mut self, addr: usize) -> Option<[u8; STRIPE_SIZE]> {
        if self.is_abort {
            return None;
        }
        assert_eq!(addr & (STRIPE_SIZE - 1), 0);

        if !self.mem.test_not_modify(addr, self.read_ver) {
            self.is_abort = true;
            return None;
        }

        fence(Ordering::Acquire);

        let mut mem = [0; STRIPE_SIZE];
        for (dst, src) in mem
            .iter_mut()
            .zip(self.mem.mem[addr..addr + STRIPE_SIZE].iter())
        {
            *dst = *src;
        }

        fence(Ordering::SeqCst);

        if !self.mem.test_not_modify(addr, self.read_ver) {
            self.is_abort = true;
            return None;
        }

        Some(mem)
    }
}

pub struct WriteTrans<'a> {
    read_ver: u64,
    read_set: HashSet<usize>,
    write_set: HashMap<usize, [u8; STRIPE_SIZE]>,
    locked: Vec<usize>,
    is_abort: bool,
    mem: &'a mut Memory,
}

impl<'a> Drop for WriteTrans<'a> {
    fn drop(&mut self) {
        for &addr in &self.locked {
            self.mem.unlock_addr(addr);
        }
    }
}

impl<'a> WriteTrans<'a> {
    fn new(mem: &'a mut Memory) -> Self {
        Self {
            read_set: HashSet::new(),
            write_set: HashMap::new(),
            locked: Vec::new(),
            is_abort: false,
            read_ver: mem.global_clock.load(Ordering::Acquire),
            mem,
        }
    }

    pub fn store(&mut self, addr: usize, val: [u8; STRIPE_SIZE]) {
        assert_eq!(addr & (STRIPE_SIZE - 1), 0);
        self.write_set.insert(addr, val);
    }

    pub fn load(&mut self, addr: usize) -> Option<[u8; STRIPE_SIZE]> {
        if self.is_abort {
            return None;
        }

        assert_eq!(addr & (STRIPE_SIZE - 1), 0);

        self.read_set.insert(addr);

        if let Some(&m) = self.write_set.get(&addr) {
            return Some(m);
        }

        if !self.mem.test_not_modify(addr, self.read_ver) {
            self.is_abort = true;
            return None;
        }

        fence(Ordering::Acquire);

        let mut mem = [0; STRIPE_SIZE];
        for (dst, src) in mem
            .iter_mut()
            .zip(self.mem.mem[addr..addr + STRIPE_SIZE].iter())
        {
            *dst = *src;
        }

        fence(Ordering::SeqCst);

        if !self.mem.test_not_modify(addr, self.read_ver) {
            self.is_abort = true;
            return None;
        }

        Some(mem)
    }

    fn lock_write_set(&mut self) -> bool {
        for (&addr, _) in self.write_set.iter() {
            if self.mem.lock_addr(addr) {
                self.locked.push(addr);
            } else {
                return false;
            }
        }
        true
    }

    fn validate_read_set(&self) -> bool {
        for &addr in &self.read_set {
            if self.write_set.contains_key(&addr) {
                let ver = self.mem.get_addr_ver(addr);
                if ver > self.read_ver {
                    return false;
                }
            } else if !self.mem.test_not_modify(addr, self.read_ver) {
                return false;
            }
        }
        true
    }

    fn commit(&mut self, ver: u64) {
        for (&addr, val) in &self.write_set {
            for (dst, src) in self.mem.mem[addr..addr + STRIPE_SIZE].iter_mut().zip(val) {
                *dst = *src;
            }
        }

        fence(Ordering::Release);

        for &addr in self.write_set.keys() {
            let idx = addr >> Memory::SHIFT_SIZE;
            self.mem.lock_ver[idx].store(ver, Ordering::Relaxed);
        }

        self.locked.clear();
    }
}

pub enum STMResult<T> {
    Ok(T),
    Retry,
    Abort,
}

pub struct STM {
    mem: UnsafeCell<Memory>,
}

unsafe impl Sync for STM {}
unsafe impl Send for STM {}

impl STM {
    pub fn new() -> Self {
        Self {
            mem: UnsafeCell::new(Memory::new()),
        }
    }

    pub fn read_transaction<R>(&self, f: impl Fn(&mut ReadTrans) -> STMResult<R>) -> Option<R> {
        loop {
            let mut tr = ReadTrans::new(unsafe { &*self.mem.get() });

            match f(&mut tr) {
                STMResult::Abort => return None,
                STMResult::Retry => {
                    if tr.is_abort {
                        continue;
                    }
                    return None;
                }
                STMResult::Ok(val) => {
                    if tr.is_abort {
                        continue;
                    } else {
                        return Some(val);
                    }
                }
            }
        }
    }

    pub fn write_tansaction<R>(&self, f: impl Fn(&mut WriteTrans) -> STMResult<R>) -> Option<R> {
        loop {
            let mut tr = WriteTrans::new(unsafe { &mut *self.mem.get() });

            let result = match f(&mut tr) {
                STMResult::Abort => return None,
                STMResult::Retry => {
                    if tr.is_abort {
                        continue;
                    }
                    return None;
                }
                STMResult::Ok(val) => {
                    if tr.is_abort {
                        continue;
                    }
                    val
                }
            };

            if !tr.lock_write_set() {
                continue;
            }

            let ver = 1 + tr.mem.inc_global_clock();

            if tr.read_ver + 1 != ver && !tr.validate_read_set() {
                continue;
            }

            tr.commit(ver);

            return Some(result);
        }
    }
}

impl Default for STM {
    fn default() -> Self {
        Self::new()
    }
}
