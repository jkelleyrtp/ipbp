//! A linker that combines changed incremental artifacts to a dylib that we load with interpositon

use std::{io::prelude::Write, time::SystemTime};

fn main() {
    let args = std::env::args().collect::<Vec<String>>();

    // panic!("This is a test panic");

    // write to a file
    let now = std::time::SystemTime::now();
    let now = now.duration_since(std::time::UNIX_EPOCH).unwrap();
    let now = now.as_secs();
    let mut log_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(format!("linker_artifacts/args-{}.txt", now))
        .unwrap();

    // Let's fingerprint since the last time we ran this binary
    let last_run = std::fs::read_to_string("linker_artifacts/fingerprint.txt").unwrap();
    let last_run = last_run.trim().parse::<u64>().unwrap();

    // write the new fingerprint
    std::fs::write("linker_artifacts/fingerprint.txt", now.to_string()).unwrap();

    // Kill the old dylib
    _ = std::fs::remove_file("linker_artifacts/patch.so");

    let mut object_files = vec![];

    for arg in args {
        println!("arg: {}", arg);

        if let Ok(file) = std::fs::File::open(&arg) {
            // Our project binary...
            // load the file and get its changed time
            // if the file changed, we'll load it as a tiny patch
            let metadata = file.metadata().unwrap();
            let changed_time = metadata
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let did_change = changed_time > last_run;

            log_file
                .write(format!("{} {:?} {did_change}\n", arg, changed_time).as_bytes())
                .unwrap();
        } else {
            log_file.write(format!("{}\n", arg).as_bytes()).unwrap();
        }

        if arg.ends_with(".o") {
            if arg.contains("symbols.o") {
                continue;
            }

            // if changed > last_run {
            //     log_file
            //         .write(format!("incremental patched: {} {:?}\n", arg, changed).as_bytes())
            //         .unwrap();
            // }

            object_files.push(arg.clone());
        }

        // A binary coming from a dependency...
        // We want to look for "dioxus" entries in the tables of these deps - if they have the name "dioxus" in them
        // This should be a one-time cost when the rlib is compiled for the first time, provided we're
        // caching and the rlibs aren't coming from anywhere
        if arg.ends_with(".rlib") {
            // object_files.push(arg.clone());
        }
    }

    let outname = format!("patch-dioxus.so");
    // let outname = format!("patch-dioxus.so", now);
    // let outname = format!("patch-{}.so", now);
    std::fs::write("linker_artifacts/patch_name.txt", outname.as_bytes()).unwrap();

    // link together just the incremental artifacts
    let mut cmd = std::process::Command::new("cc");

    // cc -dynamiclib -undefined dynamic_lookup -arch arm64 -o linker_artifacts/patch.so
    cmd.arg("-dylib")
        .arg("-undefined")
        .arg("dynamic_lookup")
        .arg("-arch")
        .arg("arm64")
        .arg("-o")
        .arg(format!("linker_artifacts/{outname}"));

    // attach the object files
    for obj in object_files {
        cmd.arg(obj);
    }

    let output = cmd.output().unwrap();

    log_file.write_all(&output.stdout).unwrap();
    log_file.write_all(&output.stderr).unwrap();
}

// .arg("-L")
// .arg("/Users/jonkelley/Development/Tinkering/ipbp/target/aarch64-apple-darwin/debug/deps")
// .arg("-framework")
// .arg("AppKit")
// .arg("-framework")
// .arg("Foundation")
// .arg("-framework")
// .arg("CoreServices")
// .arg("-framework")
// .arg("Carbon")
// .arg("-framework")
// .arg("CoreGraphics")
// .arg("-framework")
// .arg("CoreFoundation")
// .arg("-framework")
// .arg("AppKit")
// .arg("-framework")
// .arg("WebKit")
// .arg("-framework")
// .arg("ApplicationServices")
// .arg("-framework")
// .arg("CoreGraphics")
// .arg("-framework")
// .arg("Carbon")
// .arg("-framework")
// .arg("CoreVideo")
// .arg("-framework")
// .arg("CoreFoundation")
// .arg("-lSystem")
// .arg("-framework")
// .arg("AppKit")
// .arg("-framework")
// .arg("QuartzCore")
// .arg("-framework")
// .arg("Foundation")
// .arg("-framework")
// .arg("CoreGraphics")
// .arg("-framework")
// .arg("CoreGraphics")
// .arg("-framework")
// .arg("CoreFoundation")
// .arg("-lSystem")
// .arg("-lobjc")
// .arg("-lobjc")
// .arg("-liconv")
// .arg("-lSystem")
// .arg("-lc")
// .arg("-lm")
// .arg("-L")
// .arg("/Users/jonkelley/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/aarch64-apple-darwin/lib")

// .arg("-Wl,-dead_strip")
// .arg("-nodefaultlibs");
