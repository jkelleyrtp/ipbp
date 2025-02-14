use std::{env, fs, path::PathBuf};

use dioxus::prelude::*;
use libloading::Library;
use memmap::MmapOptions;
use object::{Object, ObjectSymbol};
use tokio::time::Instant;

/// loop, check for changes to source, reload, and patch
pub fn use_background_hotreloader(fnc: fn() -> Element) -> Element {
    let library = use_signal(|| None as Option<Library>);
    println!("rendering");

    use_future(move || async move {
        let mut source = None;
        let mut patched_lib: Option<Library> = None as Option<Library>;
        // let mut fn_ptr = my_fn_ptr.as_mut() as *mut dyn Fn() -> Element;

        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            patch_with_forward_symbol(library, &mut source, &mut patched_lib);
        }
    });

    library.with(|f| {
        if let Some(lib) = f {
            println!("Using patched library");
            let fn_ptr = unsafe { lib.get::<unsafe extern "C" fn() -> Element>(b"some_func_1") };
            let el = unsafe { fn_ptr.unwrap()() };
            println!("it's patched!");
            el
        } else {
            fnc()
        }
    })
}

pub fn patch_with_forward_symbol<'a>(
    mut fn_ptr: Signal<Option<Library>>,
    src: &mut Option<String>,
    lib: &'a mut Option<Library>,
) -> bool {
    // print pwd
    // println!("Current directory: {:?}", env::current_dir().unwrap());

    // println!("comparing...");
    let contents = std::fs::read_to_string("packages/hr/src/main.rs").unwrap();

    // Load the original source
    if src.is_none() {
        *src = Some(contents);
        return false;
    }

    let previous = src.as_ref().unwrap();

    // If the source has changed, we need to reload the binary, otherwise keep chugging
    if previous == &contents {
        return false;
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
    let now = Instant::now();

    let _out =
        std::process::Command::new("/Users/jonkelley/Development/Tinkering/ipbp/direct_rustc.sh")
            .output()
            .unwrap();

    // let patch_name = std::str::from_utf8(&_out.stdout).unwrap().trim();
    println!("linking complete... took {:?}", now.elapsed().as_millis());

    println!("dlopening...");

    // we need to change the name of the libary since it will clobber itself when we load it over and over
    // load the name from the linker artifacts dir
    let patch_name = std::fs::read_to_string("linker_artifacts/patch_name.txt").unwrap();

    println!("patch name: {:?}", patch_name.trim());

    // Now we need to load in the symbols from the new binary and patch them in
    let so = PathBuf::from(format!("linker_artifacts/{}", patch_name.trim()));

    // lets load the library and then try to resolve its missing symbols from our process
    let fread = fs::read(&so).unwrap();
    let obj = object::File::parse(&fread as &[u8]).unwrap();

    // Open ourself as a file too
    let file = std::fs::File::open(env::current_exe().unwrap()).unwrap();
    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };
    let us_obj = object::File::parse(&*mmap).unwrap();

    let mut missing_symbols = vec![];
    for sym in obj.symbols() {
        if sym.is_undefined() {
            // println!("looking for symbol: {:?}", sym.name().unwrap());
            if us_obj.symbol_by_name(sym.name().unwrap()).is_none() {
                missing_symbols.push(sym.name().unwrap());
                println!("Symbol not found on import: {:?}", sym.name().unwrap());
            }
        }
    }

    // if !missing_symbols.is_empty() {
    //     println!("couldn't load lib - missing symbols {:#?}", missing_symbols);
    //     return false;
    // }

    // dyld[31223]: dlopen("linker_artifacts/patch.dylib", 0x00000005)
    // dyld[31223]: <A1F97656-1CB1-3ADB-B9F1-D32A51F24928> /Users/jonkelley/Development/Tinkering/ipbp/linker_artifacts/patch.dylib
    // dyld[31223]: dyld_image_path_containing_address(0x127148000) => '/Users/jonkelley/Development/Tinkering/ipbp/linker_artifacts/patch.dylib'
    // dyld[31223]: _dyld_is_memory_immutable(0x127148000, 28) => 0
    // dyld[31223]: dyld_image_path_containing_address(0x127148000) => '/Users/jonkelley/Development/Tinkering/ipbp/linker_artifacts/patch.dylib'
    // dyld[31223]:       dlopen(patch.dylib) => 0x8e08c600
    // unloading...
    // dyld[31223]: dlclose(0x8e08cb80)
    // dyld[31223]: _dyld_get_image_uuid(0x126388000, 0x16baad030)
    // acquring symbol...
    // dyld[31223]: dlerror()dyld[31223]:  => NULL
    // dyld[31223]: dlsym(0x8e08c600, "some_func_1")
    // dyld[31223]:      dlsym("some_func_1") => 0x12717232c

    // unsafe {
    let mut _lib = unsafe { libloading::Library::new(so).ok().unwrap() };
    let old = fn_ptr.replace(Some(_lib));

    if let Some(old) = old {
        println!("unloading...");

        // old.close().unwrap();
        std::mem::forget(old);

        // std::mem::forget(old);
        // old.close().unwrap();
        println!("unloaded!");

        // for now just unload it...
        // std::mem::forget(old)
    }

    // let aold = lib.replace(_lib);

    // if let Some(old) = old {
    //     println!("unloading...");

    //     // std::mem::forget(old);
    //     old.close().unwrap();
    //     println!("unloaded!");

    //     // for now just unload it...
    //     // std::mem::forget(old)
    // }

    // let new: &'a Library = lib.as_ref().unwrap();

    // println!("acquring symbol...");
    // let func: libloading::Symbol<'a, fn() -> Element> = new.get(b"some_func_1").unwrap();

    // println!("patched in {:?}", now.elapsed().as_millis());

    // we want to make sure there's no symbols in this library that are unresolved
    // we're going to try and resolve them from the root executable

    // let raw_ptr = func.into_raw().into_raw();

    // println!("raw ptr of new sym {:x}", raw_ptr as usize);

    // println!("raw ptr of old sym {:x}", *fn_ptr as *const () as usize);

    // fn_ptr = std::mem::transmute(raw_ptr);
    // let old = fn_ptr.replace(Box::new(move || {
    //     let func: fn() -> Element = unsafe { std::mem::transmute(raw_ptr) };
    //     func()
    // }));

    // std::mem::forget(old);

    // *fn_ptr = *func;

    // *lib = Some(_lib);

    // now forget the library so it doesn't get unloaded before we're dong with it
    // std::mem::forget(lib);
    // };

    true
}
