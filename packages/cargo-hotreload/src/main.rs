use std::{path::PathBuf, process::Stdio, time::SystemTime};

use anyhow::Context;
use cargo_metadata::camino::Utf8PathBuf;
use clap::Parser;
use futures::StreamExt;
use notify::{event::DataChange, Watcher};
use serde::Deserialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::{Child, Command},
    time::Instant,
};

mod diff;

#[derive(Debug, Parser)]
enum Args {
    #[clap(name = "hotreload")]
    Hotreload,

    #[clap(name = "diff")]
    Diff,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Go through the linker if we need to
    if let Ok(action) = std::env::var("HOTRELOAD_LINK") {
        return link(action).await;
    }

    // Otherwise the commands
    match Args::parse() {
        Args::Hotreload => hotreload_loop().await,
        Args::Diff => diff::main().await,
    }
}

async fn hotreload_loop() -> anyhow::Result<()> {
    // Save the state of the rust files
    let main_rs = PathBuf::from(workspace_root().join("packages/harness/src/main.rs"));
    let mut contents = std::fs::read_to_string(&main_rs).unwrap();

    // Modify the main.rs mtime so we skip "fresh" builds
    // Basically `touch main.rs` in the directory
    std::fs::File::open(&main_rs)?.set_modified(SystemTime::now())?;

    let cur_exe = std::env::current_exe()?;
    let now = std::time::Instant::now();
    let inital_build = Command::new("cargo")
        .arg("rustc")
        .arg("--package")
        .arg("harness")
        .arg("--bin")
        .arg("harness")
        .arg("--profile")
        .arg("hotreload")
        .arg("--message-format")
        .arg("json-diagnostic-rendered-ansi")
        .arg("--verbose")
        .arg("--")
        .arg(format!("-Clinker={}", cur_exe.canonicalize()?.display()))
        .env("HOTRELOAD_LINK", "start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let CargoOutputResult {
        output_location: exe,
        direct_rustc,
    } = run_cargo_output(inital_build, false).await?;
    println!("Initial build complete in: {:?}", now.elapsed());

    // copy the exe and give it a "fat" name
    let now = std::time::SystemTime::UNIX_EPOCH;
    let fat_exe = exe.with_file_name(format!("fatharness-{}", now.elapsed().unwrap().as_millis()));
    std::fs::copy(&exe, &fat_exe)?;

    // Launch the fat exe. We'll overwrite the slim exe location, so this prevents the app from bugging out
    let mut app = Command::new(fat_exe)
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut app_stdin = app.stdin.take().unwrap();

    let (tx, mut rx) = futures_channel::mpsc::unbounded();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        _ = tx.unbounded_send(res);
    })?;

    watcher.watch(&main_rs, notify::RecursiveMode::NonRecursive)?;

    while let Some(Ok(event)) = rx.next().await {
        if event.kind
            != notify::EventKind::Modify(notify::event::ModifyKind::Data(DataChange::Content))
        {
            continue;
        }

        let new_contents = std::fs::read_to_string(&main_rs).unwrap();
        if new_contents == contents {
            println!("File changed but contents didn't change");
            continue;
        }
        contents = new_contents;

        println!("Fast reloading... ");

        let fast_build = Command::new(direct_rustc[0].clone())
            .args(direct_rustc[1..].iter())
            .env("HOTRELOAD_LINK", "reload")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // going through rustc directly
        // .arg("rustc")
        // .arg("--package")
        // .arg("harness")
        // .arg("--bin")
        // .arg("harness")
        // .arg("--profile")
        // .arg("hotreload")
        // .arg("--message-format")
        // .arg("json-diagnostic-rendered-ansi")
        // .arg("--verbose")
        // .arg("--")
        // .arg(format!("-Clinker={}", cur_exe.canonicalize()?.display()))
        // .arg(format!("-Cdebuginfo=0"))

        let started = Instant::now();
        let output = run_cargo_output(fast_build, false).await;
        let output = match output {
            Ok(output) => output.output_location,
            Err(e) => {
                println!("cargo failed: {e:?}");
                continue;
            }
        };

        let output_temp =
            output.with_file_name(format!("output-{}", now.elapsed().unwrap().as_millis()));
        std::fs::copy(&output, &output_temp).unwrap();

        println!("output: {:?}", output_temp);

        // write the new object file to the stdin of the app
        app_stdin
            .write_all(format!("{}\n", output_temp).as_bytes())
            .await?;
        println!("took {:?}", started.elapsed());
    }

    drop(app);

    Ok(())
}

async fn link(action: String) -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<String>>();

    std::fs::write(workspace_root().join("link.txt"), args.join("\n"))?;

    match action.as_str() {
        // This is the first time we're running the linker - don't strip any symbols. we want them there during hot-reloads
        "start" => {
            let args = args
                .into_iter()
                .skip(1)
                .filter(|arg| arg != "-Wl,-dead_strip")
                .collect::<Vec<String>>();

            let object_files: Vec<_> = args.iter().filter(|arg| arg.ends_with(".o")).collect();
            cache_incrementals(object_files.as_ref());

            // Run ld with the args
            let res = Command::new("cc").args(args).output().await?;
            let err = String::from_utf8_lossy(&res.stderr);
            std::fs::write(workspace_root().join("link_errs.txt"), &*err).unwrap();

            return Ok(());
        }

        // This is a hot-reload. Don't rebuild with any .rlib files.
        // Eventually, perform a smarter analysis
        "reload" => {
            // let hotreload_dir = workspace_root().join("target").join("cargo-hotreload");
            // std::fs::create_dir_all(&hotreload_dir).unwrap();
            let index_of_out = args.iter().position(|arg| arg == "-o").unwrap();
            let out_file = args[index_of_out + 1].clone();
            let object_files: Vec<_> = args.iter().filter(|arg| arg.ends_with(".o")).collect();

            cache_incrementals(object_files.as_ref());

            // -O0 ? supposedly faster
            // -reproducible - even better?
            // -exported_symbol and friends - could help with dead-code stripping
            // -e symbol_name - for setting the entrypoint
            // -keep_relocs ?

            // run the linker, but unexport the `_main` symbol
            let res = Command::new("cc")
                .args(object_files)
                .arg("-dylib")
                .arg("-undefined")
                .arg("dynamic_lookup")
                .arg("-Wl,-unexported_symbol,_main")
                .arg("-arch")
                .arg("arm64")
                .arg("-dead_strip") // maybe?
                .arg("-o")
                .arg(&out_file)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await?;

            let err = String::from_utf8_lossy(&res.stderr);
            std::fs::write(workspace_root().join("link_errs.txt"), &*err).unwrap();
        }

        _ => panic!("don't know"),
    }

    Ok(())
}

/// Move all previous object files to "incremental-old" and all new object files to "incremental-new"
fn cache_incrementals(object_files: &[&String]) {
    let old = workspace_root().join("data").join("incremental-old");
    let new = workspace_root().join("data").join("incremental-new");

    // Remove the old incremental-old directory if it exists
    _ = std::fs::remove_dir_all(&old);

    // Rename incremental-new to incremental-old if it exists. Faster than moving all the files
    _ = std::fs::rename(&new, &old);

    // Create the new incremental-new directory to place the outputs in
    std::fs::create_dir_all(&new).unwrap();

    // Now drop in all the new object files
    for o in object_files.iter() {
        if !o.ends_with(".rcgu.o") {
            continue;
        }

        let path = PathBuf::from(o);
        std::fs::copy(&path, new.join(path.file_name().unwrap())).unwrap();
    }
}

fn workspace_root() -> PathBuf {
    "/Users/jonkelley/Development/Tinkering/ipbp".into()
}

struct CargoOutputResult {
    output_location: Utf8PathBuf,
    direct_rustc: Vec<String>,
}

async fn run_cargo_output(
    mut child: Child,
    should_render: bool,
) -> anyhow::Result<CargoOutputResult> {
    let stdout = tokio::io::BufReader::new(child.stdout.take().unwrap());
    let stderr = tokio::io::BufReader::new(child.stderr.take().unwrap());
    let mut output_location = None;
    let mut stdout = stdout.lines();
    let mut stderr = stderr.lines();

    let mut direct_rustc = vec![];

    loop {
        use cargo_metadata::Message;

        let line = tokio::select! {
            Ok(Some(line)) = stdout.next_line() => line,
            Ok(Some(line)) = stderr.next_line() => line,
            else => break,
        };

        let mut messages = Message::parse_stream(std::io::Cursor::new(line));

        loop {
            let message = match messages.next() {
                Some(Ok(message)) => message,
                None => break,
                other => {
                    println!("other: {other:?}");
                    break;
                }
            };

            match message {
                Message::CompilerArtifact(artifact) => {
                    if let Some(i) = artifact.executable {
                        output_location = Some(i)
                    }
                }
                Message::CompilerMessage(compiler_message) => {
                    if let Some(rendered) = &compiler_message.message.rendered {
                        if should_render {
                            println!("rendered: {rendered}");
                        }
                    }
                }
                Message::BuildScriptExecuted(_build_script) => {}
                Message::BuildFinished(build_finished) => {
                    if !build_finished.success {
                        // assuming we received a message from the compiler, so we can exit
                        anyhow::bail!("Build failed");
                    }
                }
                Message::TextLine(word) => {
                    if word.trim().starts_with("Running ") {
                        // trim everyting but the contents between the quotes
                        let args = word
                            .trim()
                            .trim_start_matches("Running `")
                            .trim_end_matches('`');

                        direct_rustc.extend(shell_words::split(args).unwrap());
                    }

                    if let Ok(artifact) = serde_json::from_str::<RustcArtifact>(&word) {
                        if artifact.emit == "link" {
                            output_location =
                                Some(Utf8PathBuf::from_path_buf(artifact.artifact).unwrap());
                        }
                    }

                    if should_render {
                        println!("text: {word}")
                    }
                }
                _ => {}
            }
        }
    }

    let output_location =
        output_location.context("Failed to find output location. Build must've failed.")?;

    Ok(CargoOutputResult {
        output_location,
        direct_rustc,
    })
}

#[derive(Debug, Deserialize)]
struct RustcArtifact {
    artifact: PathBuf,
    emit: String,
}
