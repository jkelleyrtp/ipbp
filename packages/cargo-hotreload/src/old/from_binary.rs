//! okay, for now, using the whole crate graph infra is annoying.
//!
//! Attempt number 5 but this time operating on the final output object.
//!
//! There will be no relocations here - we should maybe consider emitting them?
//! We will load the new binary into our symbol space manually, emulating dlopen, and then patch up
//!
//! - thread locals
//! - statics
//! - functions (that are marked as hot-reloadable)
//!
//! This is not so different from the dylib approach, but instead of swapping symbols, we are just
//! legit going to patch things.
//!
//! We could do some more complex static analysis on the final binary so there's not much code to change.

use memmap::MmapOptions;
use object::{
    read::{ReadCache, ReadRef},
    ObjectSection, ObjectSymbol, RelocationTarget,
};
use object::{File, Object};
use std::{
    collections::HashMap,
    env, fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[test]
fn attempt_to_dlopen_ourself() {
    let file = std::fs::File::open("../../target/aarch64-apple-darwin/debug/hr_base").unwrap();

    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };

    let image_start = mmap.as_ptr() as usize;

    let mut object = File::parse(&*mmap).unwrap();

    // for sym in object.symbols() {
    //     if let Ok(name) = sym.name() {
    //         println!("{name}");
    //     }
    // }

    // Init sections
    let mut sections: Vec<_> = object.sections().map(|s| Section::new(&s)).collect();

    // Identify initializer and finalizer list
    let init = Vec::new();
    let fini = Vec::new();
    for _sec in &sections {
        // let _f = InitFini::new(_sec);
    }

    // Update runtime address and allocate space for bss
    for sec in &mut sections {
        sec.update_runtime_addr(image_start)?;

        // log::trace!(
        //     "Section '{}' loaded at: [0x{:0x}, 0x{:0x} + {})",
        //     sec.name,
        //     sec.address,
        //     sec.address,
        //     sec.size
        // );
    }
}
