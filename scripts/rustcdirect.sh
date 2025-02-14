/Users/jonkelley/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc \
    --crate-name harness \
    --edition=2021 \
    packages/harness/src/main.rs \
    --error-format=json \
    --json=diagnostic-rendered-ansi,artifacts,future-incompat \
    --diagnostic-width=200 \
    --crate-type bin \
    --emit=dep-info,link \
    -C embed-bitcode=no \
    --check-cfg 'cfg(docsrs)' \
    --check-cfg 'cfg(feature, values())' \
    -C metadata=67f0cb47d0f65770 \
    -C extra-filename=-67f0cb47d0f65770 \
    --out-dir /Users/jonkelley/Development/Tinkering/ipbp/target/aarch64-apple-darwin/debug/deps \
    --target aarch64-apple-darwin \
    -C incremental=/Users/jonkelley/Development/Tinkering/ipbp/target/aarch64-apple-darwin/debug/incremental \
    -C strip=debuginfo \
    -L dependency=/Users/jonkelley/Development/Tinkering/ipbp/target/aarch64-apple-darwin/debug/deps \
    -L dependency=/Users/jonkelley/Development/Tinkering/ipbp/target/debug/deps
