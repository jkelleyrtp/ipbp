use std::{path::PathBuf, process::Stdio};

use anyhow::Context;
use cargo_metadata::camino::Utf8PathBuf;
use futures::StreamExt;
use notify::{event::DataChange, Watcher};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt},
    process::{Child, Command},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if let Ok(action) = std::env::var("HOTRELOAD_LINK") {
        return link(action).await;
    }

    let cur_exe = std::env::current_exe()?;

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
        .arg("--")
        .arg(format!("-Clinker={}", cur_exe.canonicalize()?.display()))
        .env("HOTRELOAD_LINK", "start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let exe = run_cargo_output(inital_build).await?;

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

    let main_rs = PathBuf::from(workspace_root().join("packages/harness/src/main.rs"));
    watcher.watch(&main_rs, notify::RecursiveMode::NonRecursive)?;

    while let Some(Ok(event)) = rx.next().await {
        if event.kind
            != notify::EventKind::Modify(notify::event::ModifyKind::Data(DataChange::Content))
        {
            continue;
        }

        println!("Fast reloading...");

        let fast_build = Command::new("cargo")
            .arg("rustc")
            .arg("--package")
            .arg("harness")
            .arg("--bin")
            .arg("harness")
            .arg("--profile")
            .arg("hotreload")
            .arg("--message-format")
            .arg("json-diagnostic-rendered-ansi")
            .arg("--")
            .arg(format!("-Clinker={}", cur_exe.canonicalize()?.display()))
            .env("HOTRELOAD_LINK", "reload")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let Ok(output) = run_cargo_output(fast_build).await else {
            continue;
        };

        let output_temp =
            output.with_file_name(format!("output-{}", now.elapsed().unwrap().as_millis()));
        std::fs::copy(&output, &output_temp).unwrap();

        println!("output: {:?}", output_temp);

        // write the new object file to the stdin of the app
        app_stdin
            .write_all(format!("{}\n", output_temp).as_bytes())
            .await?;
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
            let object_files = args.iter().filter(|arg| arg.ends_with(".o"));

            let res = Command::new("cc")
                .args(object_files)
                .arg("-dylib")
                .arg("-undefined")
                .arg("dynamic_lookup")
                .arg("-arch")
                .arg("arm64")
                .arg("-o")
                .arg(&out_file)
                .output()
                .await?;
            let err = String::from_utf8_lossy(&res.stderr);
            std::fs::write(workspace_root().join("link_errs.txt"), &*err).unwrap();
        }

        _ => panic!("don't know"),
    }

    Ok(())
}

fn workspace_root() -> PathBuf {
    "/Users/jonkelley/Development/Tinkering/ipbp".into()
}

async fn run_cargo_output(mut child: Child) -> anyhow::Result<Utf8PathBuf> {
    let stdout = tokio::io::BufReader::new(child.stdout.take().unwrap());
    let stderr = tokio::io::BufReader::new(child.stderr.take().unwrap());
    let mut output_location = None;
    let mut stdout = stdout.lines();
    let mut stderr = stderr.lines();

    loop {
        use cargo_metadata::Message;

        let line = tokio::select! {
            Ok(Some(line)) = stdout.next_line() => line,
            Ok(Some(line)) = stderr.next_line() => line,
            else => break,
        };

        let Some(Ok(message)) = Message::parse_stream(std::io::Cursor::new(line)).next() else {
            continue;
        };

        match message {
            Message::CompilerArtifact(artifact) => {
                if let Some(i) = artifact.executable {
                    output_location = Some(i)
                }
            }
            Message::CompilerMessage(compiler_message) => {
                if let Some(rendered) = compiler_message.message.rendered {
                    println!("{rendered}");
                }
            }
            Message::BuildScriptExecuted(_build_script) => {}
            Message::BuildFinished(build_finished) => {
                if !build_finished.success {
                    // assuming we received a message from the compiler, so we can exit
                    anyhow::bail!("Build failed");
                }
            }
            Message::TextLine(word) => println!("{word}"),
            _ => {}
        }
    }

    let output_location =
        output_location.context("Failed to find output location. Build must've failed.")?;

    Ok(output_location)
}
