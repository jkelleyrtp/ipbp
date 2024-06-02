//! Introspect the running process for its symbols
//! We're gonna try and do some wizadry to statically link the incoming binary code *into* this running process.
//! This means relocations need to point to already-resolved symbols.
//!
//! To do that, we need to parse our own address space as a macho-o file, and then resolve our own symbols.
//!
//!

use dioxus::prelude::Element;
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
fn print_our_global_symbols() {
    let exe = PathBuf::from("../../target/aarch64-apple-darwin/debug/harness")
        .canonicalize()
        .unwrap();

    let patch_target = PathBuf::from("../../saved/arrow/3smwra9nz79z3yg9.o");

    patch_file(exe, patch_target);
}

fn patch_file(exe: PathBuf, with: PathBuf) {
    // let exe = env::current_exe().unwrap();
    // let exe = env::current_exe().unwrap();

    // harness up the binary from another running process...
    // dbg!(PathBuf::from("../../target/aarch64-apple-darwin/")
    //     .canonicalize()
    //     .unwrap());
    // let mut file = fs::File::open(exe).unwrap();
    // let data = fs::read(exe).unwrap();

    // let object = File::parse(&*data).unwrap();

    // let cache = ReadCache::new(file);
    // let data = cache.range(0, cache.len().unwrap());

    let file = std::fs::File::open(exe).unwrap();
    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };

    let mut object = File::parse(&*mmap).unwrap();

    // rust symbol name -> address
    let mut saved_rust_symbols = HashMap::new();

    for sym in object.symbols() {
        // if sym.name().unwrap().starts_with("__Z") {
        // demangle the rust symbols
        // let demangled = rustc_demangle::demangle(sym.name().unwrap());
        saved_rust_symbols.insert(sym.name().unwrap(), sym.address());
        // }
    }

    // Load the symbols from the patch file...
    let contents = fs::read(with).unwrap();
    let in_obj = object::read::File::parse(&contents as &[u8]).unwrap();

    // // Now let's verify all the undefined symbols in the patch file are in our binary
    for sym in in_obj.symbols() {
        if sym.is_undefined() {
            let name = sym.name().unwrap();
            if !saved_rust_symbols.contains_key(name) {
                println!("Symbol not found: {:?}", name);
            }
        }
    }

    // Lets walk all the relocations, resolve their symbols, and then see if we can find them in our binary
    for section in in_obj.sections() {
        for (offset, relocation) in section.relocations() {
            match relocation.target() {
                RelocationTarget::Symbol(sym) => {
                    let symbol = in_obj.symbol_by_index(sym).unwrap();
                    let name = symbol.name().unwrap();
                    if symbol.is_undefined() {
                        if !saved_rust_symbols.contains_key(name) {
                            println!("Symbol from relocation not found: {:?}", name);
                        }
                    }
                }

                // these are basically got/plt entries
                RelocationTarget::Section(_) => {}
                RelocationTarget::Absolute => {}
                _ => {}
            }
        }
    }

    // Now print the symbols this object file is exporting - we're gonna try and patch them in...
    for sym in in_obj.symbols() {
        if !sym.is_undefined() {
            let name = sym.name().unwrap();
            if saved_rust_symbols.contains_key(name) {
                println!(
                    "Symbol already defined, gonna attempt a patch: {:?}",
                    sym.name().unwrap()
                );

                // _some_funcs for now are our indication that this symbol can be hotreloaded.
                // we're going to steal its address after having loaded it in and then patch it in.
                // This involves rewiring the existing function in memory to point to the new one, using
                // a trampoline / jump table.
                if name.starts_with("_some_func") {
                    let addr = *saved_rust_symbols.get(name).unwrap();
                    println!("Existing resolved address for some_func: {:x}", addr);

                    // Now we need to patch in the new address...
                }
            }
        }
    }
}

/// Read the target directory looking for the most recently changed .o file since the last time we checkeda
fn load_bins(name: &str) -> Vec<PathBuf> {
    // print pwd
    println!("Current directory: {:?}", env::current_dir().unwrap());

    let dir = PathBuf::from("target/aarch64-apple-darwin/debug/incremental")
        .canonicalize()
        .unwrap();

    // find the most recently changed folder that starts with the name

    let mut bins = vec![];
    let last_folder = changed_folder_recent(dir, name);

    let path = last_folder.unwrap();

    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        if entry.path().is_dir() {
            for entry in fs::read_dir(entry.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.path().is_file() && entry.path().extension().unwrap() == "o" {
                    // println!("Checking directory: {:?}", entry.path());
                    // let metadata = entry.metadata().unwrap();
                    bins.push(entry.path());
                }
            }
        }
    }

    bins
}

#[test]
fn blah() {
    let o = changed_folder_recent(
        PathBuf::from("../../target/aarch64-apple-darwin/debug/incremental"),
        "hr",
    );

    dbg!(o);
}

fn changed_folder_recent(dir: PathBuf, name: &str) -> Option<PathBuf> {
    let mut last = UNIX_EPOCH;
    let mut last_folder = None;

    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(name)
            {
                // don't consume dirs with a folder that has no .o files in it
                let mut dir = fs::read_dir(&path).unwrap();
                let first_dir = dir
                    .find(|entry| entry.as_ref().unwrap().path().is_dir())
                    .unwrap()
                    .unwrap();
                let mut dir = fs::read_dir(first_dir.path()).unwrap();
                if dir
                    .find(|entry| entry.as_ref().unwrap().path().extension().unwrap() == "o")
                    .is_none()
                {
                    continue;
                }

                let meta = fs::metadata(&path).unwrap();
                let modified = meta.modified().unwrap();
                if modified > last {
                    last = modified;
                    last_folder = Some(path);
                }
            }
        }
    }

    last_folder
}

pub type ObjFiles = HashMap<String, Vec<u8>>;

pub fn attempt_patch(files: &mut ObjFiles) {
    if files.is_empty() {
        for file in load_bins("hr") {
            let data = fs::read(&file).unwrap();
            files.insert(file.to_str().unwrap().to_string(), data);
        }
        return;
    }

    let bins = load_bins("hr");

    if bins.is_empty() {
        panic!(
            "No files found in the target directory - {:?}",
            changed_folder_recent(
                PathBuf::from("target/aarch64-apple-darwin/debug/incremental")
                    .canonicalize()
                    .unwrap(),
                "hr"
            )
        )
    }

    // println!(
    //     "Checking for changes in the following files: {:?} -> {:?}",
    //     files.keys().collect::<Vec<_>>(),
    //     bins
    // );

    // Diff each file, and if it's changed, reload it.
    for bin in bins {
        let data = fs::read(&bin).unwrap();
        let key = bin.to_str().unwrap().to_string();

        if !files.contains_key(&key) {
            // panic!("New file detected: {:?}", bin);
            println!("New file detected: {:?}", bin);
            // continue;
        }

        let old = files.get(&key).unwrap();

        if old != &data {
            println!("Attempting to patch into process: {:?}", bin);
            patch_file(std::env::current_exe().unwrap(), bin);
        }
    }
}

fn load_depgraph() {}

pub fn patch_with_forward_symbol(fn_ptr: &mut fn() -> Element, src: &mut Option<String>) {
    // print pwd
    // println!("Current directory: {:?}", env::current_dir().unwrap());

    // println!("comparing...");
    let contents = std::fs::read_to_string("packages/hr/src/main.rs").unwrap();

    // Load the original source
    if src.is_none() {
        *src = Some(contents);
        return;
    }

    let previous = src.as_ref().unwrap();

    // If the source has changed, we need to reload the binary, otherwise keep chugging
    if previous == &contents {
        return;
    }

    // Update the source
    *src = Some(contents);

    // Patch the binary by compiling this project with --emit=obj
    // we're going to forward our symbols from the root executable into this thin executable
    // We might want to strip down this binary in the future such that 1) it's tiny and 2) it refuses
    // to bring in any dependent resolved symbols.a
    // What we *might* want is the rlib format - I'm not yet sure if this approach brings in dependencies or not

    // Run the cargo build, giving us the thin object file that doesn't have its own symbols resovled yet
    println!("incr building...");
    let _out = std::process::Command::new("cargo")
        .arg("rustc")
        .arg("--package")
        .arg("hr")
        .arg("--")
        .arg("--emit=obj")
        .arg("-o")
        .arg("prod_comp/hr_prod")
        // .arg("prod_comp/hr_prod_pre")
        .output()
        .unwrap();

    // Rust will mark this with a fingerprint, so we actually need to find the first file that matches the pattern
    let fprint = find_finger_print().unwrap();

    // We want to obj copy and strip out statics/tls such that they get inherited by their parent process
    // We'll eventually achieve this by walking the symbol table and removing all the statics/tls
    // For now, just choose one static to remove and see if it works
    // let _out = std::process::Command::new("~/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/bin/llvm-objcopy")
    //     .arg("--strip-symbol")
    //     .arg("SHARED_SYMBOL")
    //     .arg(format!("prod_comp/hr_prod_pre-{}.o", fprint))
    //     .arg(format!("prod_comp/hr_prod-{}.o", fprint))
    //     .output()
    //     .unwrap();

    println!("man linking...");
    // we want to link it such that it's a shared object, and we can load it in and patch it in
    // cc -dylib  -undefined dynamic_lookup -arch arm64 hr_obj-a7e3b91a98f994df.o -o prod_comp/hr_prod_dylib
    let _out = std::process::Command::new("cc")
        .arg("-dylib")
        .arg("-undefined")
        .arg("dynamic_lookup")
        .arg("-arch")
        .arg("arm64")
        .arg(format!("prod_comp/hr_prod-{}.o", fprint))
        .arg("-o")
        .arg("prod_comp/hr_prod_dylib.so")
        .output()
        .unwrap();

    println!("dlopening...");

    // Now we need to load in the symbols from the new binary and patch them in
    let so = PathBuf::from("prod_comp/hr_prod_dylib.so");
    unsafe {
        let lib = libloading::Library::new(so).unwrap();

        println!("acquring symbol...");
        let func: libloading::Symbol<fn() -> Element> = lib.get(b"some_func_1").unwrap();

        // let func: libloading::Symbol<fn()> = lib.get(b"usaaa").unwrap();
        // func();
        // let func: libloading::Symbol<fn()> = lib.get(b"usaaa").unwrap();
        // func();
        *fn_ptr = *func;

        // now forget the library so it doesn't get unloaded
        std::mem::forget(lib);
    };
}

fn find_finger_print() -> Option<String> {
    let dir = std::fs::read_dir("prod_comp").unwrap();

    for file in dir {
        let file = file.unwrap();
        let path = file.path();
        if let Some(ext) = path.extension() {
            if ext == "o" {
                // make sure it matches this pattern
                // hr_prod-<fingerprint>.o
                // We're just gonna rip off the fingerprint by prasing to the next `.`
                let stem = path.file_stem().unwrap();
                let stem_as_str = stem.to_str().unwrap();
                let fingerprint = stem_as_str
                    .trim_start_matches("hr_prod-")
                    .split('.')
                    .next()
                    .unwrap();

                return Some(fingerprint.to_string());
            }
        }
    }

    None
}

#[test]
fn find_o_file_test() {
    _ = env::set_current_dir("/Users/jonkelley/Development/Tinkering/ipbp/");

    dbg!(find_finger_print());
}
