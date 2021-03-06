use std::{arch::asm, cell::UnsafeCell, ptr::null_mut};

#[repr(C)]
struct Node<T> {
    next: *mut Node<T>,
    data: T,
}

#[repr(C)]
pub struct StackHead<T> {
    head: *mut Node<T>,
}

impl<T> StackHead<T> {
    fn new() -> Self {
        Self { head: null_mut() }
    }

    pub fn push(&mut self, v: T) {
        let node = Box::new(Node {
            next: null_mut(),
            data: v,
        });

        let ptr = Box::into_raw(node) as *mut u8 as usize;

        let head = &mut self.head as *mut *mut Node<T> as *mut u8 as usize;

        unsafe {
            asm!(
                "1:
                ldxr {next}, [{head}]
                str {next}, [{ptr}]
                stlxr w10, {ptr}, [{head}]
                cbnz w10, 1b",
                next = out(reg) _,
                ptr = in(reg) ptr,
                head = in(reg) head,
                out("w10") _
            )
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        unsafe {
            let head = &mut self.head as *mut *mut Node<T> as *mut u8 as usize;
            let mut result: usize;

            asm!(
                "1:
                ldaxr {result}, [{head}]
                cbnz {result}, 2f

                clrex
                b 3f

                2:
                ldr {next}, [{result}]
                stxr w10, {next}, [{head}]
                cbnz w10, 1b

                3:",
                next = out(reg) _,
                result = out(reg) result,
                head = in(reg) head,
                out("w10") _
            );

            if result == 0 {
                None
            } else {
                let ptr = result as *mut u8 as *mut Node<T>;
                let head = Box::from_raw(ptr);
                Some((*head).data)
            }
        }
    }
}

impl<T> Drop for StackHead<T> {
    fn drop(&mut self) {
        let mut node = self.head;
        while !node.is_null() {
            let n = unsafe { Box::from_raw(node) };
            node = n.next;
        }
    }
}

pub struct Stack<T> {
    data: UnsafeCell<StackHead<T>>,
}

impl<T> Stack<T> {
    pub fn new() -> Self {
        Self {
            data: UnsafeCell::new(StackHead::new()),
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn get_mut(&self) -> &mut StackHead<T> {
        unsafe { &mut *self.data.get() }
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Self {
        Self::new()
    }
}

unsafe impl<T> Sync for Stack<T> {}
unsafe impl<T> Send for Stack<T> {}
