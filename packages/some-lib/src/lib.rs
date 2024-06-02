use std::{cell::RefCell, ops::Deref};

#[no_mangle]
static mut SHARED_SYMBOL: usize = 0;

pub fn read_shared_symbol() -> usize {
    unsafe { SHARED_SYMBOL }
}

pub fn write_shared_symbol(val: usize) {
    unsafe {
        SHARED_SYMBOL = val;
    }
}

thread_local! {
    pub static A_THREAD_LOCAL : RefCell<u64> =  RefCell::new(0);
}

pub fn set_thread_local(val: u64) {
    A_THREAD_LOCAL.with(|refcell| {
        refcell.replace(val);
    })
}

pub fn get_thread_local() -> u64 {
    A_THREAD_LOCAL.with(|refcell| *refcell.borrow().deref())
}
