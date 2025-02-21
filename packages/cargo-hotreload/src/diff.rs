use anyhow::Result;
use memmap::{Mmap, MmapOptions};
use std::{
    borrow,
    cmp::Ordering,
    collections::VecDeque,
    env, error,
    ffi::OsStr,
    fs,
    marker::PhantomData,
    ops::Deref,
    path::{self, PathBuf},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::Path,
};

use itertools::Itertools;
use object::{
    macho::MachHeader64,
    read::macho::{MachOFile, MachOSection, MachOSymbol, Nlist},
    Endianness, Export, File, Import, Object, ObjectSection, ObjectSegment, ObjectSymbol,
    ObjectSymbolTable, ReadRef, Relocation, RelocationTarget, SectionIndex, SectionKind,
    SymbolIndex, SymbolKind,
};
use pretty_assertions::Comparison;
use pretty_hex::{Hex, HexConfig};

#[tokio::test]
async fn works() {
    main().await.unwrap();
}

pub async fn main() -> anyhow::Result<()> {
    ObjectDiff::new().unwrap().load();
    Ok(())
}

struct ObjectDiff {
    old: Vec<LoadedFile>,
    new: Vec<LoadedFile>,
    matched: usize,
    mismatched: usize,
    missing: usize,
    modified_files: HashMap<PathBuf, HashSet<String>>,
    modified_symbols: HashSet<String>,
    // parent -> child
    deps: HashMap<String, HashSet<String>>,
    // child -> parent
    parents: HashMap<String, HashSet<String>>,
}

impl ObjectDiff {
    fn new() -> Result<Self> {
        Ok(Self {
            old: LoadedFile::from_dir(&workspace_dir().join("data").join("incremental-old"))?,
            new: LoadedFile::from_dir(&workspace_dir().join("data").join("incremental-new"))?,
            matched: 0,
            mismatched: 0,
            missing: 0,
            modified_files: Default::default(),
            modified_symbols: Default::default(),
            deps: Default::default(),
            parents: Default::default(),
        })
    }

    fn load(&mut self) -> Result<()> {
        for x in 0..self.new.len() {
            self.load_file(x)?;
        }

        println!("matched: {}", self.matched);
        println!("mismatched: {}", self.mismatched);
        println!("missing: {}", self.missing);
        // println!("modified: {:#?}", self.modified_files);
        // println!("changed chain: {:#?}", self.modified_symbols);

        for (parent, children) in self.deps.iter() {
            for child in children {
                self.parents
                    .entry(child.to_string())
                    .or_default()
                    .insert(parent.to_string());
            }
        }

        for changed in self.modified_symbols.iter() {
            self.print_parent(changed);
        }

        // print the call graph from "_main"
        // self.print_call_graph("_main", 0);

        Ok(())
    }

    fn print_parent(&self, name: &str) {
        let mut stack = vec![(name.to_string(), 0)];
        let mut seen = HashSet::new();

        while let Some((current_name, idx)) = stack.pop() {
            if !seen.insert(current_name.clone()) {
                continue;
            }
            if idx > 30 {
                continue;
            }

            for _ in 0..idx {
                print!(" ");
            }
            println!(" {}", current_name);

            if let Some(parents) = self.parents.get(&current_name) {
                for parent in parents {
                    stack.push((parent.to_string(), idx + 1));
                }
            }
        }
    }

    // Find the path from this symbol to the root
    // fn find_parents(&self, name: &str, path_to_root: &mut HashSet<String>) {
    //     if let Some(parents) = self.parents.get(name) {
    //         for parent in parents {
    //             if path_to_root.insert(parent.to_string()) {
    //                 self.find_parents(&parent, path_to_root);
    //             }
    //         }
    //     }
    // }

    fn print_call_graph(&self, name: &str, idx: usize) {
        if idx > 200 {
            return;
        }

        for _ in 0..idx {
            print!(" *");
        }
        println!(" {name}");
        if let Some(children) = self.deps.get(name) {
            for child in children {
                self.print_call_graph(child, idx + 1);
            }
        }
    }

    fn load_file(&mut self, idx: usize) -> Result<()> {
        use object::read::File;

        let num_left = self.old.len();
        let num_right = self.new.len();

        let new_file = &self.new[idx];
        let left = self
            .old
            .iter()
            .find(|l| l.path.file_name() == new_file.path.file_name());

        let Some(left) = left else {
            println!("no left for {:?}", new_file.path);
            self.modified_files
                .entry(new_file.path.clone())
                .or_default();

            return Ok(());
        };

        println!(
            "----- {:?} {}/{} -----",
            new_file.path.file_name(),
            idx,
            num_right
        );

        // We need to deal with macho directly for now... eventually
        let File::MachO64(old_) = File::parse(&left.mmap.deref() as &[u8])? else {
            panic!()
        };
        let File::MachO64(new_) = File::parse(&new_file.mmap.deref() as &[u8])? else {
            panic!()
        };

        // Ok("__text")
        // Ok("__gcc_except_tab")
        // Ok("__const")
        // Ok("__const")
        // Ok("__literal16")
        // Ok("__literal8")

        let dep_graph = ModuleWithRelocations::new(&new_);

        for section in new_.sections() {
            let n = section.name().unwrap();
            if n == "__text" || n == "__const" || n.starts_with("__literal") {
                let changed = self.acc_changed(&old_, &new_, section.index());
                // println!("changed: {:#?}", changed);
                if !changed.is_empty() {
                    println!("section: {n}");
                }
                for n in &changed {
                    let parents = dep_graph.acc_public_parents(n);

                    self.modified_symbols
                        .extend(parents.iter().map(|p| p.to_string()));

                    self.modified_files
                        .entry(new_file.path.clone())
                        .or_default()
                        .extend(parents.iter().map(|p| p.to_string()));
                }
            }
        }

        Ok(())
    }

    fn acc_changed(
        &self,
        old: &MachOFile<'_, MachHeader64<Endianness>>,
        new: &MachOFile<'_, MachHeader64<Endianness>>,
        section_idx: SectionIndex,
    ) -> HashSet<String> {
        let mut local_modified = HashSet::new();

        // Accumulate modified symbols using masking in functions
        let relocated_new = acc_symbols(&new, section_idx);
        let mut relocated_old = acc_symbols(&old, section_idx)
            .into_iter()
            .map(|f| (f.name, f))
            .collect::<HashMap<_, _>>();

        for right in relocated_new {
            // let is_exported = new.exports.contains_key(&right.name);

            // temp assert while in dev
            let Some(left) = relocated_old.remove(right.name) else {
                local_modified.insert(right.name.to_string());
                // println!("no right for {}", right.name);
                continue;
            };

            match compare_masked(old, new, &left, &right) {
                true => {}
                false => {
                    println!(
                        "Sym [{} ] - {:?}",
                        right.sym.address(),
                        // if is_exported { "export" } else { "local" },
                        right.sym.name().unwrap(),
                        // pretty_hex::config_hex(
                        //     &right.data,
                        //     HexConfig {
                        //         display_offset: right.sym.address() as usize,
                        //         ..Default::default()
                        //     },
                        // )
                    );
                    println!("❌ Symbols do not match");
                    println!();

                    // names might be different, insert both
                    local_modified.insert(left.name.to_string());
                    local_modified.insert(right.name.to_string());
                }
            }
        }

        local_modified
    }
}

struct LoadedFile {
    path: PathBuf,
    file: std::fs::File,
    mmap: Mmap,
}

impl LoadedFile {
    fn from_dir(dir: &Path) -> anyhow::Result<Vec<Self>> {
        let dir = std::fs::read_dir(dir)?;
        let mut files = dir
            .flatten()
            .map(|f| f.path())
            .filter(|p| p.extension() == Some(OsStr::new("o")))
            .map(|p| Self::new(p))
            .collect::<Result<Vec<_>, _>>()?;

        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(files)
    }

    fn new(path: PathBuf) -> anyhow::Result<Self> {
        let file = std::fs::File::open(&path)?;
        let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };
        Ok(Self { path, file, mmap })
    }
}

type FileRef<'data> = &'data MachOFile<'data, MachHeader64<Endianness>>;

struct Computed<'data, 'file> {
    file: FileRef<'data>,
    exports: HashMap<&'data str, Export<'data>>,
    imports: HashMap<&'data str, Import<'data>>,
    _phantom: PhantomData<&'file ()>,
    // text: MachOSection<'data, 'file, MachHeader64<Endianness>>,
    // text_data: &'data [u8],
    // sorted_functions: Vec<MachOSymbol<'data, 'file, MachHeader64<Endianness>>>,
}

impl<'data, 'file> Computed<'data, 'file> {
    fn new(file: &'data MachOFile<'data, MachHeader64<Endianness>>) -> Self {
        let exports = file
            .exports()
            .unwrap()
            .iter()
            .map(|e| (e.name().to_utf8(), *e))
            .collect::<HashMap<_, _>>();

        let imports = file
            .imports()
            .unwrap()
            .iter()
            .map(|i| (i.name().to_utf8(), *i))
            .collect::<HashMap<_, _>>();

        Self {
            file,
            exports,
            imports,
            _phantom: PhantomData,
        }
    }
}

fn stable_sort_symbols(
    a: &MachOSymbol<MachHeader64<Endianness>>,
    b: &MachOSymbol<MachHeader64<Endianness>>,
) -> Ordering {
    let addr = a.address().cmp(&b.address());
    if addr == Ordering::Equal {
        a.index().0.cmp(&b.index().0)
    } else {
        addr
    }
}

/// A function with its relevant relocations to be used for masked comparisons
struct RelocatedSymbol<'a> {
    name: &'a str,
    /// offset within the section
    offset: usize,
    data: &'a [u8],
    relocations: &'a [(u64, Relocation)],
    sym: MachOSymbol<'a, 'a, MachHeader64<Endianness>>,
    section: SectionIndex,
}

fn acc_symbols<'a>(new: FileRef<'a>, section_idx: SectionIndex) -> Vec<RelocatedSymbol<'a>> {
    let mut syms = vec![];

    let section = new.section_by_index(section_idx).unwrap();

    let sorted = new
        .symbols()
        .filter(|s| s.section_index() == Some(section_idx))
        .sorted_by(stable_sort_symbols)
        .collect::<Vec<_>>();

    // todo!!!!!! jon: don't leak this lol
    let relocations = section
        .relocations()
        .sorted_by(|a, b| a.0.cmp(&b.0).reverse())
        .collect::<Vec<_>>()
        .leak();

    let data = section.data().unwrap();

    // no data? no symbols
    if data.is_empty() {
        return vec![];
    }

    // No symbols, no symbols,
    if sorted.is_empty() {
        return vec![];
    }

    // The end of the currently analyzed function
    let mut func_end = section.size() as usize;

    // The idx into the relocation list that applies to this function. We'll march these
    let mut reloc_idx = 0;

    // Walk in reverse so we can use the text_length as the initial backstop and to match relocation order
    for sym in sorted.into_iter().rev() {
        let sym_offset = sym.address() - section.address();

        // Move the head/tail to include the sub-slice of the relocations that apply to this symbol
        let mut reloc_start = None;
        loop {
            // If we've reached the end of the relocations then we're done
            if reloc_idx == relocations.len() {
                break;
            }

            // relocations behind the symbol start don't apply
            if relocations[reloc_idx].0 < sym_offset {
                break;
            } else {
            }

            // Set the head to the first relocation that applies
            if reloc_start.is_none() {
                reloc_start = Some(reloc_idx);
            }

            reloc_idx += 1;
        }

        // Identify the instructions that apply to this symbol
        let data_range = sym_offset as usize..func_end;
        let data = &data[data_range.clone()];

        // Identify the relocations that apply to this symbol
        let relocations = match reloc_start {
            Some(start) => &relocations[start..reloc_idx],
            None => &[],
        };

        syms.push(RelocatedSymbol {
            sym,
            name: sym.name().unwrap(),
            offset: sym_offset as usize,
            data,
            relocations,
            section: section_idx,
        });

        func_end = (sym_offset) as usize;
    }

    assert_eq!(reloc_idx, relocations.len());

    syms
}

/// Compare two sets of bytes, masking out the bytes that are not part of the symbol
/// This is so we can compare functions with different relocations
fn compare_masked<'a>(
    old: &impl Object<'a>,
    new: &impl Object<'a>,
    left: &RelocatedSymbol,
    right: &RelocatedSymbol,
) -> bool {
    // Make sure the relocations are the same length
    if left.relocations.len() != right.relocations.len() {
        return false;
    }

    // Make sure the data is the same length
    // If the size changed then the instructions are different (well, not necessarily, but enough)
    if left.data.len() != right.data.len() {
        return false;
    }

    // Make sure the names match
    if left.name != right.name {
        println!("sym name doesn't: {:?} != {:?}", left.name, right.name);
        return false;
    }

    // We're going to walk from relocation target to target, but since there's no implicit target
    // to start with, we simply use the end of the data
    let mut last = left.data.len();

    // Ensure the relocations point to the same symbol
    // Data symbols are special ... todo
    //
    // relocations are in reverse order, so we can also compare the data as we go
    for x in 0..left.relocations.len() {
        // Grab the reloc
        let (l_addr, left_reloc): &(u64, Relocation) = &left.relocations[x];
        let (_r_addr, right_reloc): &(u64, Relocation) = &right.relocations[x];

        // The targets might not be same by index but should resolve to the same *name*
        let left_target: RelocationTarget = left_reloc.target();
        let right_target: RelocationTarget = right_reloc.target();

        // Use the name of the symbol to compare
        // todo: decide if it's internal vs external
        let left_name = symbol_name_of_relo(old, left_target).unwrap();
        let right_name = symbol_name_of_relo(new, right_target).unwrap();

        // Make sure the names match
        if left_name != right_name {
            // if the target is a locally defined symbol, then it might be the same
            // todo(jon): hash the masked contents
            println!("reloc target doesn't match: {left_name:?} != {right_name:?}");
            return false;
        }

        // Check the data
        // the slice is the end of the relocation to the start of the previous relocation
        let reloc_byte_size = (left_reloc.size() as usize) / 8;
        let start = *l_addr as usize - left.offset as usize + reloc_byte_size;
        // println!(
        //     "addr: {l_addr}, adju: {}, start: {start}, last: {last}",
        //     *l_addr as usize - left.offset
        // );
        debug_assert!(start <= last);
        debug_assert!(start <= left.data.len());

        let left_subslice = &left.data[start..last];
        let right_subslice = &right.data[start..last];

        if left_subslice != right_subslice {
            return false;
        }

        // todo: more checking... the symbols might be local
        last = start - reloc_byte_size;
    }

    // And a final check to make sure the data is the same
    if left.data[..last] != right.data[..last] {
        return false;
    }

    true
}

fn symbol_name_of_relo<'a>(obj: &impl Object<'a>, target: RelocationTarget) -> Option<&'a str> {
    match target {
        RelocationTarget::Symbol(symbol_index) => Some(
            obj.symbol_by_index(symbol_index)
                .unwrap()
                .name_bytes()
                .unwrap()
                .to_utf8(),
        ),
        RelocationTarget::Section(_) => None,
        RelocationTarget::Absolute => {
            println!("Absolute relocation target");
            None
        }
        _ => None,
    }
}

fn workspace_dir() -> PathBuf {
    "/Users/jonkelley/Development/Tinkering/ipbp".into()
}

trait ToUtf8<'a> {
    fn to_utf8(&self) -> &'a str;
}

impl<'a> ToUtf8<'a> for &'a [u8] {
    fn to_utf8(&self) -> &'a str {
        std::str::from_utf8(self).unwrap()
    }
}

struct CachedObjectFile {
    path: PathBuf,
    exports: HashSet<String>,
}

type DepGraph = HashMap<SymbolIndex, HashSet<SymbolIndex>>;

#[test]
fn does_have_exports() {
    let f =
        "/Users/jonkelley/Development/Tinkering/ipbp/target/hotreload/deps/output-1740094105568";
    let f = PathBuf::from(f);
    let data = fs::read(&f).unwrap();

    let File::MachO64(old_) = object::read::File::parse(&data as &[u8]).unwrap() else {
        panic!()
    };

    for export in old_.exports().unwrap() {
        println!("{:?}", export.name().to_utf8());
    }
}

#[derive(Default)]
struct ModuleWithRelocations<'a> {
    // name -> symbol
    sym_tab: HashMap<&'a str, RelocatedSymbol<'a>>,

    // symbol -> symbols
    deps: HashMap<&'a str, HashSet<&'a str>>,

    // symbol -> symbols
    parents: HashMap<&'a str, HashSet<&'a str>>,
}

impl<'a> ModuleWithRelocations<'a> {
    fn new(new: FileRef<'a>) -> Self {
        let mut m = ModuleWithRelocations::default();

        // Build the symbol table
        for sect in new.sections() {
            for r in acc_symbols(&new, sect.index()) {
                m.sym_tab.insert(r.name, r);
            }
        }

        let text_section = new.section_by_name_bytes(b"__text").unwrap();
        let text_index = text_section.index();
        let text_syms_by_addres = new
            .symbols()
            // .filter(|s| s.section_index() == Some(text_index))
            // .filter(|s| s.section_index() == Some(text_index))
            // .filter(|s| s)
            .filter(|s| s.is_definition())
            .map(|s| (s.address(), s.name().unwrap()))
            .collect::<BTreeMap<_, _>>();

        // println!("text_syms_by_addres: {:#?}", text_syms_by_addres);

        // println!(
        //     "checking section {} at address {}",
        //     text_section.name().unwrap(),
        //     text_section.address()
        // );

        // Build the call graph by walking the relocations
        for (sym_name, sym) in m.sym_tab.iter() {
            let entry = m.deps.entry(sym_name).or_default();
            let sym_section = new.section_by_index(sym.section).unwrap();
            let sym_data = sym_section.data().unwrap();

            for (_addr, reloc) in sym.relocations.iter() {
                let target = match symbol_name_of_relo(new, reloc.target()) {
                    Some(name) => name,
                    None => {
                        let RelocationTarget::Section(section_index) = reloc.target() else {
                            panic!("no target for {sym_name}");
                        };

                        let offset = *_addr as usize;
                        // println!("reloc: {reloc:?}");

                        let value_bytes = &sym_data[offset as usize..(offset + 8) as usize];
                        let addend = u64::from_le_bytes([
                            value_bytes[0],
                            value_bytes[1],
                            value_bytes[2],
                            value_bytes[3],
                            value_bytes[4],
                            value_bytes[5],
                            value_bytes[6],
                            value_bytes[7],
                        ]);

                        // let target_section = new.file.section_by_index(section_index).unwrap();

                        // let data = sym.data;
                        // let data = section.data().unwrap();
                        // println!(
                        //     "target section {} at address {} for relo addr {}",
                        //     section.name().unwrap(),
                        //     section.address(),
                        //     *_addr
                        // );
                        // if *_addr as usize <= section.address() as usize {
                        //     println!(
                        //         "Bad reloc: {sym_name} -> {}, {_addr} is before {}\n{:?}",
                        //         section.name().unwrap(),
                        //         section.address(),
                        //         reloc
                        //     );
                        // }

                        // let offset = *_addr as usize;
                        // let offset = *_addr as usize - section.address() as usize;

                        // println!("value: {:?}", addend);
                        // println!("value corrected: {:?}", addend - target_section.address());
                        // let value = addend - target_section.address();

                        // let o = ;
                        // // println!("{} -> {:?}", sym_name, o);
                        // o
                        text_syms_by_addres.get(&addend).unwrap()
                    }
                };

                entry.insert(target);
            }
        }

        // Build the parent graph
        for (parent, children) in m.deps.iter() {
            for child in children {
                m.parents.entry(child).or_default().insert(parent);
            }
        }

        Self {
            sym_tab: m.sym_tab,
            deps: m.deps,
            parents: m.parents,
        }
    }

    fn acc_public_parents(&self, name: &'a str) -> Vec<&'a str> {
        let mut roots = vec![];

        let mut stack = vec![(name, 0)];
        let mut seen = HashSet::new();

        while let Some((current_name, idx)) = stack.pop() {
            if !seen.insert(current_name.clone()) {
                continue;
            }

            let entry = self.sym_tab.get(current_name).unwrap();
            let parents = self.parents.get(current_name);

            if entry.sym.is_global() {
                roots.push(current_name);
            }

            if let Some(parents) = parents {
                for parent in parents {
                    stack.push((parent, idx + 1));
                }
            }
        }

        roots
    }

    fn print_parents(&self, name: &str) {
        let mut stack = vec![(name.to_string(), 0)];
        let mut seen = HashSet::new();

        while let Some((current_name, idx)) = stack.pop() {
            if !seen.insert(current_name.clone()) {
                continue;
            }
            if idx > 30 {
                continue;
            }

            for _ in 0..idx {
                print!(" ");
            }

            let entry = self.sym_tab.get(current_name.as_str()).unwrap();
            let parents = self.parents.get(current_name.as_str());
            let has_any_parents = parents.map(|p| !p.is_empty()).unwrap_or(false);

            println!(
                " {} - {} {} {} {}",
                current_name,
                if entry.sym.is_global() {
                    "global"
                } else {
                    "local"
                },
                entry.offset,
                entry.section,
                if !has_any_parents { "❌" } else { "" }
            );

            if let Some(parents) = parents {
                for parent in parents {
                    stack.push((parent.to_string(), idx + 1));
                }
            }
        }
    }
}

// Ok("__text")
// Ok("__const")
// Ok("__const")
// Ok("__gcc_except_tab")
// Ok("__compact_unwind")
// Ok("__eh_frame")
//
/// I think symbols show up in data sections and need to be identified
#[test]
fn hmm_imports() {
    let f = "/Users/jonkelley/Development/Tinkering/ipbp/data/incremental-old/harness-df4868ea1b5cadad.3c2tm4jj9yl4umdxtid0wfenb.rcgu.o";
    let f = PathBuf::from(f);

    let data = fs::read(&f).unwrap();

    let File::MachO64(new) = object::read::File::parse(&data as &[u8]).unwrap() else {
        panic!()
    };

    let graph = ModuleWithRelocations::new(&new);

    for sec in new.sections() {
        println!("{:?}", sec.name());
    }

    //    Options for introspecting the linker
    //  -why_load
    //          Log why each object file in a static library is loaded. That is, what symbol was needed.  Also called -whyload for compatibility.

    //  -why_live symbol_name
    //          Logs a chain of references to symbol_name.  Only applicable with -dead_strip .  It can help debug why something that you think should be dead strip removed is not removed.  See
    //          -exported_symbols_list for syntax and use of wildcards.

    //  -print_statistics
    //          Logs information about the amount of memory and time the linker used.

    //  -t      Logs each file (object, archive, or dylib) the linker loads.  Useful for debugging problems with search paths where the wrong library is loaded.

    //  -order_file_statistics
    //          Logs information about the processing of a -order_file.

    //  -map map_file_path
    //          Writes a map file to the specified path which details all symbols and their addresses in the output image.

    // let lsym = old.file.symbol_by_name("ltmp7").unwrap();
    // println!(
    //     "{:?} {:?} {:?} {:?} {:?}",
    //     lsym.address(),
    //     lsym.name(),
    //     lsym.kind(),
    //     lsym.section(),
    //     old.file
    //         .section_by_index(lsym.section_index().unwrap())
    //         .unwrap()
    //         .name()
    // );

    // for export in old.file.exports().unwrap() {
    //     println!("{:?} {:?}", export.address(), export.name().to_utf8());
    // }

    // for sym in old.file.symbols() {
    //     if sym.index() == lsym.index() {
    //         println!("{:?} {:?}", sym.address(), sym.name());
    //     }

    //     // if sym.address() == lsym.address() {
    //     // println!("{:?} {:?}", sym.address(), sym.name());
    //     // }
    //     // println!("{:?} {:?}", sym.address(), sym.name());
    // }

    // println!("{:#?}", graph.deps);
    // println!("{:#?}", graph.parents["__ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8c5c4774279549f9E"]);
    // println!("{:#?}", graph.deps["ltmp6"]);
    // println!("{:#?}", graph.parents["ltmp6"]);
}

#[test]
fn where_are_you_used() {
    let f= "/Users/jonkelley/Development/Tinkering/ipbp/data/incremental-new/harness-df4868ea1b5cadad.egexdem49eejgynie32zwugcu.rcgu.o";
    let f = PathBuf::from(f);
    let data = fs::read(&f).unwrap();
    let File::MachO64(new) = object::read::File::parse(&data as &[u8]).unwrap() else {
        panic!()
    };
    let mut old = Computed::new(&new);
    let graph = ModuleWithRelocations::new(&new);
    let imports = new.imports().unwrap();
    let im = imports
        .iter()
        .find(|e| {
            e.name().to_utf8()
                == "__ZN7harness13zoom_controls28_$u7b$$u7b$closure$u7d$$u7d$17h7a3c177fab0c21c7E"
        })
        .unwrap();

    for (dep, children) in graph.deps.iter() {
        if children.contains(im.name().to_utf8()) {
            println!("{dep:?}");
        }
    }

    // for import in new.imports().unwrap() {
    //     println!("{:?}", import.name().to_utf8());
    // }
}

#[test]
fn build_internal_call_graph() {
    let f = "/Users/jonkelley/Development/Tinkering/ipbp/data/incremental-new/harness-ce721ccd4e3f382d.9wlujqu2r4bjdin21bprnpfdb.rcgu.o";
    let f = PathBuf::from(f);
    let data = fs::read(&f).unwrap();
    let File::MachO64(new) = object::read::File::parse(&data as &[u8]).unwrap() else {
        panic!()
    };
    let graph = ModuleWithRelocations::new(&new);
}
