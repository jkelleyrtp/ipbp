fn main() {
    loop {
        some_func_1();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[no_mangle]
#[inline(never)]
pub fn some_func_1() {
    println!("some_func_3");
}
