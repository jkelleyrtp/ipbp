use std::{ops::Deref, ptr::read_volatile};

use const_serialize::{ConstStr, ConstVec};

#[track_caller]
pub const fn make_hot<F>(f: F) -> HotFn<F> {
    let caller = std::panic::Location::caller();

    #[derive(const_serialize::SerializeConst)]
    pub struct HotLocation {
        file: ConstStr,
        line: u32,
        column: u32,
    }

    // todo use const serialize
    #[link_section = "__DATA,__hot_fn"]
    pub static LOC: ConstVec<u8, 1024> = {
        let caller = std::panic::Location::caller();
        let buf = ConstVec::new();
        const_serialize::serialize_const(
            &HotLocation {
                file: ConstStr::new(caller.file()),
                line: caller.line(),
                column: caller.column(),
            },
            buf,
        )
    };

    HotFn {
        inner: f,
        location: caller,
        meta: &LOC,
    }
}

pub struct HotFn<T> {
    inner: T,
    location: &'static std::panic::Location<'static>,
    meta: &'static ConstVec<u8, 1024>,
}

impl<T> HotFn<T> {
    fn wrapped_call(&self) -> &T {
        let ty = std::any::type_name::<T>();
        // println!("called - {ty} -> {:#?}", self.location);
        unsafe { read_volatile(&self.meta as *const _) };
        &self.inner
    }
}

impl<T> Deref for HotFn<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        self.wrapped_call()
    }
}

trait HotCallable<Args, Return, Marker> {}

impl<A, R, F> HotCallable<A, R, ()> for F where F: Fn() -> R {}

mod tests {
    use super::*;

    fn demo() -> String {
        "hello".to_string()
    }

    fn hot_expansion() -> String {
        make_hot(|| "hello".to_string())()
    }

    // #[test]
    fn it_works() {
        let f = make_hot(demo);
        let p = f();
        println!("{p}");

        let p = hot_expansion();
    }

    #[test]
    fn main() {
        it_works();
    }
}
