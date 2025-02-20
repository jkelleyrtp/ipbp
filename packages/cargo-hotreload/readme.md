design:

- create the original binary but without stripping symbols
- fast rebuilds don't link `.rlibs` but instead use `-undefined dynamic_lookup`
- attempt to merge statics/globals within the new out file. do this by diffing the original and new object files
  - tracking statics/globals over time
  - or just bailing completely
  - diffing and remove any duplicate symbols - they should already exist
- dlopen the new library and pull out relevant symbols

things that might need to change:
- dlopen might need to be in the current namespace or we'll end up duplicating symbols
- this design requires involvement from the runtime
- can we get it to not require runtime support?

this design is good because:
- new() is tied to drop() - objects can shift in size/layout without a problem

this design is bad because:
- typeid changes for structs if their fields change even if the struct itself doesn't

notes:
- typeid is stable between compiles
- typeid does change if the contents of the struct change
- all functions will be new (by address)

https://github.com/apple-opensource/ld64/blob/e28c028b20af187a16a7161d89e91868a450cadc/src/ld/Resolver.cpp#L1154

we're going to resolve the address of duplicate symbols against the running process
