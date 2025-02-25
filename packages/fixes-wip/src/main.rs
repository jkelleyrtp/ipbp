use once_cell::sync::Lazy;
use std::{any::TypeId, marker::PhantomData};

fn main() {
    println!("Hello, world! {Thing1}");
}

static Thing1: &'static str = module_path!();

struct Thing {
    id: TypeId,
}

fn MakeThing<T: 'static>(t: PhantomData<T>) -> Thing {
    let t = TypeId::of::<T>();
    Thing { id: t }
}

const fn LazyInternal() -> &'static str {
    static IN: Lazy<i32> = Lazy::new(|| 0);
    ""
}
