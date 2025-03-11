pub use dioxus::desktop::window;
use dioxus::prelude::*;
use jumptable::JumpTableRead;
use libc::{dladdr, Dl_info};
use libloading::os::unix::{Library, RTLD_NOW};
use libloading::os::unix::{RTLD_GLOBAL, RTLD_LAZY};
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use std::{
    any::type_name_of_val,
    collections::HashMap,
    env,
    ffi::{c_void, CStr},
    fs,
    hash::Hash,
    mem::transmute_copy,
    ops::Deref,
    path::PathBuf,
    ptr::null_mut,
    sync::Arc,
};
use tokio::io::AsyncBufReadExt;

mod deref_helper;
pub mod iterate_phdr;
pub mod jumptable;
pub mod subsecond;
pub mod subsecond2;

// todo: if there's a reference held while we run our patch, this gets invalidated. should probably
// be a pointer to a jump table instead, behind a cell or something. I believe Atomic + relaxed is basically a no-op
static mut APP_JUMP_TABLE: Option<JumpTableRead> = None;
static mut HOTRELOAD_HANDLERS: Vec<Arc<dyn Fn()>> = vec![];

pub const fn current<F: SomeFn + Copy>(f: F) -> HotFn<F> {
    HotFn { inner: f }
}

pub struct HotFn<T: SomeFn> {
    inner: T,
}

impl<T: SomeFn> HotFn<T> {
    pub fn call(&self, args: T::Args) -> T::Return {
        unsafe {
            // Try to handle known function pointers. This is *really really* unsafe, but due to how
            // rust trait objects work, it's impossible to make an arbitrary usize-sized type implement Fn()
            // since that would require a vtable pointer, pushing out the bounds of the pointer size.
            if size_of::<T>() == size_of::<fn() -> ()>() {
                return self.inner.call_as_ptr(args);
            }

            // Handle trait objects. This will occur for sizes other than usize. Normal rust functions
            // become ZST's and thus their <T as SomeFn>::call becomes a function pointer to the function.
            //
            // For non-zst (trait object) types, then there might be an issue. The real call function
            // will likely end up in the vtable and will never be hot-reloaded since signature takes self.
            if let Some(jump_table) = APP_JUMP_TABLE.as_ref() {
                let known_fn_ptr = <T as SomeFn>::call as *const ();
                let ptr = jump_table.map.get(&(known_fn_ptr as u64)).unwrap().clone() as *const ();

                // https://stackoverflow.com/questions/46134477/how-can-i-call-a-raw-address-from-rust
                let _f =
                    std::mem::transmute::<*const (), fn(&T, <T as SomeFn>::Args) -> T::Return>(ptr);
                _f(&self.inner, args)
            } else {
                self.inner.call(args)
            }
        }
    }
}

pub trait SomeFn {
    type Args;
    type Return;
    type Real;

    // rust-call isnt' stable, so we wrap the underyling call with our own, giving it a stable vtable entry
    fn call(&self, args: Self::Args) -> Self::Return;

    // call this as if it were a real function pointer. This is very unsafe
    unsafe fn call_as_ptr(&self, _args: Self::Args) -> Self::Return;
}

impl<T, R> SomeFn for T
where
    T: Fn() -> R,
{
    type Args = ();
    type Return = R;
    type Real = fn() -> R;
    fn call(&self, _args: Self::Args) -> Self::Return {
        self()
    }
    unsafe fn call_as_ptr(&self, _args: Self::Args) -> Self::Return {
        let real = std::mem::transmute_copy::<Self, Self::Real>(&self);

        unsafe {
            if let Some(jump_table) = APP_JUMP_TABLE.as_ref() {
                let known_fn_ptr = real as *const ();
                let ptr = jump_table.map.get(&(known_fn_ptr as u64)).unwrap().clone() as *const ();
                let detoured = std::mem::transmute::<*const (), Self::Real>(ptr);
                detoured()
            } else {
                real()
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn hotfn_load_binary_patch__ipbp(path: *const i8, jump_table_path: *const i8) {
    let patch = PathBuf::from(unsafe { CStr::from_ptr(path).to_str().unwrap() });
    let jump_table = PathBuf::from(unsafe { CStr::from_ptr(jump_table_path).to_str().unwrap() });
    run_patch(patch, jump_table)
}

/// Run the patch
pub fn run_patch(patch: PathBuf, jump_table: PathBuf) {
    let lib = unsafe { libloading::os::unix::Library::new(PathBuf::from(patch)).unwrap() };
    let lib = Box::leak(Box::new(lib));

    // Load the jump table by deserializing it from the file
    let jump_table_path = PathBuf::from(jump_table);
    let mut jump_table: JumpTableRead =
        bincode::deserialize(&std::fs::read(jump_table_path).unwrap()).unwrap();

    let old_dl_offset = unsafe { Library::this().get::<*const ()>(b"main") }
        .unwrap()
        .as_raw_ptr()
        .wrapping_sub(jump_table.old_main_address as usize);

    // Correct the jump table since dlopen will load the binary at a different address than the original
    let new_dl_offset = unsafe { lib.get::<*const ()>(b"main") }
        .unwrap()
        .as_raw_ptr()
        .wrapping_sub(jump_table.new_main_address as usize);

    // Modify the jump table to be relative to the base address of the loaded library
    jump_table.map = jump_table
        .map
        .iter()
        .map(|(k, v)| (*k + old_dl_offset as u64, *v + new_dl_offset as u64))
        .collect();

    unsafe { APP_JUMP_TABLE = Some(jump_table) }

    // And then call the original main function
    for handler in unsafe { HOTRELOAD_HANDLERS.iter() } {
        handler();
    }
}

/// Creates a new hotreloadable function based on the incoming function. The "key" here is the caller location.
/// If that changes then a new function will be generated. This basically lets us retour function pointers
/// without annotating them.
pub const fn hotreloadable(f: fn() -> Element) -> fn() -> Element {
    static mut ORIGINAL_APP_MAIN: Option<fn() -> Element> = None;

    unsafe {
        ORIGINAL_APP_MAIN = Some(f);
    }

    // this can be simply injected to dioxus core perhaps?
    pub fn inner_reloadable() -> Element {
        // runtime integration...
        use_hook(|| unsafe { HOTRELOAD_HANDLERS.push(dioxus::prelude::schedule_update()) });

        // Calling the hot reloadable function
        current(unsafe { ORIGINAL_APP_MAIN.unwrap() }).call(())
    }

    inner_reloadable
}

// #[no_mangle]
// pub extern "C" fn hotfn_load_binary_patch__ipbp(path: *const i8, jump_table_path: *const i8) {
//     let patch = PathBuf::from(unsafe { CStr::from_ptr(path).to_str().unwrap() });
//     let jump_table = PathBuf::from(unsafe { CStr::from_ptr(jump_table_path).to_str().unwrap() });
//     run_patch(patch, jump_table)
// }

// /// Run the patch
// pub fn run_patch(patch: PathBuf, jump_table: PathBuf) {
//     let lib = unsafe { libloading::os::unix::Library::new(PathBuf::from(patch)).unwrap() };
//     let lib = Box::leak(Box::new(lib));

//     // Load the jump table by deserializing it from the file

//     let jump_table_path = PathBuf::from(jump_table);
//     let mut jump_table: JumpTableRead =
//         bincode::deserialize(&std::fs::read(jump_table_path).unwrap()).unwrap();

//     // // Correct the jump table since dlopen will load the binary at a different address than the original
//     // let old_dl_offset = unsafe { Library::this().get::<*const ()>(b"main") }
//     //     .unwrap()
//     //     .as_raw_ptr()
//     //     .wrapping_sub(jump_table.old_main_address as usize);

//     // Correct the jump table since dlopen will load the binary at a different address than the original
//     let new_dl_offset = unsafe { lib.get::<*const ()>(b"main") }
//         .unwrap()
//         .as_raw_ptr()
//         .wrapping_sub(jump_table.new_main_address as usize);

//     // println!("old_dl_offset: {old_dl_offset:?}");
//     println!("new_dl_offset: {new_dl_offset:?}");

//     // Modify the jump table to be relative to the base address of the loaded library
//     jump_table.map = jump_table
//         .map
//         .iter()
//         .map(|(k, v)| (*k, *v - new_dl_offset as u64))
//         // .map(|(k, v)| (*k - old_dl_offset as u64, *v - new_dl_offset as u64))
//         .collect();

//     unsafe { APP_JUMP_TABLE = Some(jump_table) }

//     // And then call the original main function
//     for handler in unsafe { HOTRELOAD_HANDLERS.iter() } {
//         handler();
//     }
// }

// /// Creates a new hotreloadable function based on the incoming function. The "key" here is the caller location.
// /// If that changes then a new function will be generated. This basically lets us retour function pointers
// /// without annotating them.
// pub const fn hotreloadable(f: fn() -> Element) -> fn() -> Element {
//     static mut ORIGINAL_APP_MAIN: Option<fn() -> Element> = None;

//     unsafe {
//         ORIGINAL_APP_MAIN = Some(f);
//     }

//     // this can be simply injected to dioxus core perhaps?
//     pub fn inner_reloadable() -> Element {
//         // runtime integration...
//         use_hook(|| unsafe { HOTRELOAD_HANDLERS.push(dioxus::prelude::schedule_update()) });

//         // Calling the hot reloadable function
//         current(unsafe { ORIGINAL_APP_MAIN.unwrap() }).call(())
//     }

//     inner_reloadable
// }
