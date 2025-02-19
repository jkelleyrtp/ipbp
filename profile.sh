# delete any files with the .mm_profdata extension
# find . -name "*.mm_profdata" -delete

# ensure a build without caching
touch packages/harness/src/main.rs

# run the compiler with the self-profiler
HOTRELOAD_LINK="reload" cargo +nightly rustc --package harness --bin harness --verbose -- -Z self-profile -Clinker=/Users/jonkelley/Development/Tinkering/ipbp/target/aarch64-apple-darwin/debug/cargo-hotreload

# find the .mm_prof_data file
# PROF=$(find . -name "*.mm_profdata")

# summarize the profiler data
# summarize summarize $PROF > summary.txt
