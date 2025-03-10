use binary_patch::*;
use dioxus::prelude::*;
use std::any::TypeId;
// mod testcases;
// mod extra_testcases;
use binary_patch::subsecond;

fn main() {
    let hot_fn1 = subsecond::make_hot(|| {
        println!("Four!!");
    });

    // let hot_fn2 = subsecond::make_hot(|| {
    //     println!("Two");
    // });

    // let hot_fn3 = subsecond::make_hot(|| {
    //     println!("Three");
    // });

    loop {
        hot_fn1();
        // hot_fn2();
        // hot_fn3();
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
