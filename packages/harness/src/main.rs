// a threadlocal with a runtime

// thread_local! {
//     // #[link_section = "__data,__dioxus"]
//     pub static MYVAR: std::cell::RefCell<u32> = std::cell::RefCell::new(0);
// }

fn main() {
    // println!("The status! {}", MYVAR.with(|v| *v.borrow()));

    loop {
        some_func_1();

        std::thread::sleep(std::time::Duration::from_secs(1));

        // // increment
        // MYVAR.with(|v| *v.borrow_mut() += 1);

        // std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

#[no_mangle]
#[inline(never)]
pub fn some_func_1() {
    println!("some_func_1!");
}
// #[no_mangle]
// #[inline(never)]
// pub fn some_func_1() {
//     println!("some_func_1! {}", MYVAR.with(|v| *v.borrow()));
// }

// pub const MYREVERSE: &str = concat!("dioxushr-", file!(), ":", line!(), ":", column!());

// __ZN7harness11some_func_117he09bc6835c5ac793E

// #[no_mangle]
// #[used]
// #[link_section = "__data,__dioxus"]
// pub static RVRSE: &str = concat!("dioxushr-", file!(), ":", line!(), ":", column!());

// if RVRSE.len() == 0 {
//     panic!("{}", RVRSE);
// }

// assert!(RVRSE.len() > 0);

// println!("some_func_1, {}", RVRSE);
// pub fn some_func_4() {
//     println!("some_func_4");
// }

// pub fn some_func_1() {
//     #[link(name = "dioxus_some_func_1", kind = "static")]
//     extern "C" {
//         #[no_mangle]
//         pub fn some_func_1();
//     }
//     // println!("some_func_1");
// }

// #[link(name = "dioxus_some_func_1", kind = "static")]
// pub fn some_func_2() {
//     println!("some_func_2");
// }

// #[link(name = "dioxus_some_func_2", kind = "static")]
// pub fn some_func_3() {
//     println!("some_func_3");
// }
