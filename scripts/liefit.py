#%%
import lief


#%%
libm = lief.parse("./saved/arrow/jx7vacigf9h88k6.o")
# %%
print(libm.imports)
# %%

for sym in libm.imported_symbols:
    print(sym.name)
# %%

# wipe away the imports, pretending we did relocations...

libm.imported_functions = []

# %%
