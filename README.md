# ipbp.rs - in place binary patching

similar to Live++ but for rust.

patch rust functions at runtime with magic and hacks.

certainly more limited than Live++. Doesn't handle all the edge cases. Doesn't care about shared libraries. Probably doesn't work for your usecase.

Targets supported (in order of importance):
- [ ] macos
- [ ] linux
- [ ] wasm
- [ ] ios
- [ ] android

## how it works

it doesnt yet

but if it did:
- diff object files
- figure out what exactly changed
- combine the changed object files using the dep map
- figure out affected symbols and functions                        <-- we are here in the tech tree
- package the .o files together into a single cursed dylib that tricks dlopen into not doing relocations for us
- dlopen that dylib at the *same address as the program root itself* such that our pic/pie code can work properly
- manually handle relocations at *runtime* using introspection to discover existing symbols
- spin up new statics (try to reuse strings where possible to limit memory leakage)
- tell the app that we've patched it and it should maybe try to do new stuff

and voila you have in-place binary patching for a running rust app.

Not only does completely circumvent the typical close, rebuild, relink, restart, reinitialize, resume flow, but it uses rust's *incremental compiler* *WITHOUT LINKING* - the only unnecessary cost we pay here is the compiler frontend + macro expansion. This is faster than pretty much anything else you could design.

## Current status:

We can diff object files and figure out which symbols have changed between revs. This lets us narrow down our patch creation to just the symbols in a few object files.

Todo:
- merge across more codegen units (.o)
- Respect the graph for larger projects (need to parse some random .bin files...)
- Assemble dylib with necessary hacks (missing relocations, etc)
- Dlopen said dylib with zero/limited-exports


Supposedly rust employs less "magic" making our primary hiccups:
- statics (actually static statics...)
- tls
- vtable

Doing this on mac seems relatively straightforward. Relocations are easy and all symbols are around, thanks to Rust.
Wasm should also be easy in theory, and then Linux too.
Windows scares me.
Android/ios require a networked variant of this which, again, should work in theory.


## the idyllic future


```rust


fn my_function


```

`cargo run` and change the harnessed function.

It hotreloads!

We don't implement any watcher code yet - just simply prope the directory for changes.

basically
- loop
- check if diff
- apply diff
- run function


check if diff loads the module and compares it against the last one
the changes should tell us
