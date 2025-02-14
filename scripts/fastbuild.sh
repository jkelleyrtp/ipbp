
export CARGO_MANIFEST_DIR=/Users/jonkelley/Development/dioxus

Users/jonkelley/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
    --crate-name todomvc \
    --edition=2021 \
    examples/todomvc.rs \
    --error-format=json \
    --json=diagnostic-rendered-ansi,artifacts,future-incompat \
    --crate-type bin \
    --emit=dep-info,link \
    -C embed-bitcode=no \
    --cfg 'feature="desktop"' \
    --check-cfg 'cfg(docsrs)' \
    --check-cfg 'cfg(feature, values("base64", "ciborium", "default", "desktop", "fullstack", "gpu", "http", "liveview", "mobile", "server", "web"))' \
    -C metadata=275d90357f78826d \
    -C extra-filename=-275d90357f78826d \
    --out-dir /Users/jonkelley/Development/dioxus/target/debug/examples \
    -C incremental=/Users/jonkelley/Development/dioxus/target/debug/incremental \
    -C strip=debuginfo \
    -L dependency=/Users/jonkelley/Development/dioxus/target/debug/deps \
    --extern async_std=/Users/jonkelley/Development/dioxus/target/debug/deps/libasync_std-5f4e102394710cf7.rlib \
    --extern dioxus=/Users/jonkelley/Development/dioxus/target/debug/deps/libdioxus-5f63d440c6c19489.rlib \
    --extern dioxus_ssr=/Users/jonkelley/Development/dioxus/target/debug/deps/libdioxus_ssr-821ee9a4ed331265.rlib \
    --extern form_urlencoded=/Users/jonkelley/Development/dioxus/target/debug/deps/libform_urlencoded-4536e55022b5f084.rlib \
    --extern futures_util=/Users/jonkelley/Development/dioxus/target/debug/deps/libfutures_util-49e6ec835bb77806.rlib \
    --extern rand=/Users/jonkelley/Development/dioxus/target/debug/deps/librand-aa33ac4e1f6e106a.rlib \
    --extern separator=/Users/jonkelley/Development/dioxus/target/debug/deps/libseparator-2f7c48c361604a5d.rlib \
    --extern serde=/Users/jonkelley/Development/dioxus/target/debug/deps/libserde-4e888403fe587180.rlib \
    --extern serde_json=/Users/jonkelley/Development/dioxus/target/debug/deps/libserde_json-7e75ca27f167b512.rlib \
    --extern tokio=/Users/jonkelley/Development/dioxus/target/debug/deps/libtokio-50b0edff19b96763.rlib \
    --extern wasm_split=/Users/jonkelley/Development/dioxus/target/debug/deps/libwasm_split-756d6f5b067cfeb9.rlib \
    --extern web_time=/Users/jonkelley/Development/dioxus/target/debug/deps/libweb_time-8e42c81f0d784861.rlib \
    -L native=/Users/jonkelley/Development/dioxus/target/debug/build/objc_exception-dfc002d182e11750/out
