# forward our args to the real linker located at target/aarch64-apple-darwin/patch-linker


# cargo build --package patch-linker

./target/aarch64-apple-darwin/debug/patch-linker "$@"
