use anyhow::{Context, Result};
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
use tokio::process::Command;

use itertools::Itertools;
use object::{
    macho::MachHeader64,
    read::macho::{MachOFile, MachOSection, MachOSymbol, Nlist},
    Endianness, Export, File, Import, Object, ObjectSection, ObjectSegment, ObjectSymbol,
    ObjectSymbolTable, ReadRef, Relocation, RelocationTarget, SectionIndex, SectionKind,
    SymbolIndex, SymbolKind, SymbolScope,
};
use pretty_assertions::Comparison;
use pretty_hex::{Hex, HexConfig};

#[tokio::test]
async fn works() {
    main().await.unwrap();
}

#[tokio::test]
async fn _attempt_partial_link() {
    let addr: u64 = std::fs::read_to_string(workspace_dir().join("harnessaddr.txt"))
        .unwrap()
        .parse()
        .unwrap();

    attempt_partial_link(addr, workspace_dir().join("partial.o")).await;
}

pub async fn attempt_partial_link(proc_main_addr: u64, out_path: PathBuf) {
    let mut d = ObjectDiff::new().unwrap();
    d.load().unwrap();

    let all_exports = d
        .new
        .iter()
        .flat_map(|(_, f)| f.macho.exports().unwrap())
        .map(|e| e.name().to_utf8())
        .collect::<HashSet<_>>();

    let mut adrp_imports = HashSet::new();

    let mut satisfied_exports = HashSet::new();

    let modified_symbols = d
        .modified_symbols
        .iter()
        .map(|f| f.as_str())
        .collect::<HashSet<_>>();

    for m in modified_symbols.iter() {
        if !m.starts_with("l") {
            d.print_parent(m);
        }
    }

    let mut modified = d.modified_files.iter().collect::<Vec<_>>();
    modified.sort_by(|a, b| a.0.cmp(&b.0));

    // Figure out which symbols are required from *existing* code
    // We're going to create a stub `.o` file that satisfies these by jumping into the original code via a dynamic lookup / and or literally just manually doing it
    for fil in modified.iter() {
        let f = d
            .new
            .get(fil.0.file_name().unwrap().to_str().unwrap())
            .unwrap();

        let i = f.macho.imports().unwrap();
        for i in i {
            if all_exports.contains(i.name().to_utf8()) {
                adrp_imports.insert(i.name().to_utf8());
            }
        }

        for e in f.macho.exports().unwrap() {
            satisfied_exports.insert(e.name().to_utf8());
        }
    }

    // Remove any imports that are indeed satisifed
    for s in satisfied_exports.iter() {
        adrp_imports.remove(s);
    }

    let addressed = {
        let f = "/Users/jonkelley/Development/Tinkering/ipbp/target/hotreload/harness";

        let f = PathBuf::from(f);
        let data = fs::read(&f).unwrap();

        let mut out_addressed = HashMap::new();
        let File::MachO64(old_) = object::read::File::parse(&data as &[u8]).unwrap() else {
            panic!()
        };

        let man_stm = old_.symbol_by_name_bytes(b"_main").unwrap();
        let aslr_offset = proc_main_addr - man_stm.address();

        for sym in old_.symbols() {
            let Ok(name) = sym.name() else {
                continue;
            };

            if let Some(o) = adrp_imports.take(name) {
                out_addressed.insert(o, sym.address() + aslr_offset);
            }
        }

        out_addressed
    };

    // println!("adrp imports addressed: {:#?}", addressed);
    // println!("adrp imports latent: {:#?}", adrp_imports);

    let stub = build_stub(addressed).unwrap();
    let stub_file = workspace_dir().join("stub.o");
    std::fs::write(&stub_file, stub).unwrap();

    // .arg("-r")
    // .arg("-Wl,-unexported_symbol,_main")
    let o = Command::new("cc")
        .args(modified.iter().map(|(f, _)| f))
        .arg(stub_file)
        .arg("-dylib")
        .arg("-Wl,-undefined,dynamic_lookup")
        .arg("-arch")
        .arg("arm64")
        .arg("-dead_strip")
        .arg("-o")
        .arg(out_path)
        .output()
        .await
        .unwrap();

    let err = String::from_utf8_lossy(&o.stderr);
    println!("err: {err}");
    std::fs::write(workspace_dir().join("link_errs_partial.txt"), &*err).unwrap();
}

pub async fn main() -> anyhow::Result<()> {
    ObjectDiff::new().unwrap().load();
    Ok(())
}

struct ObjectDiff {
    old: BTreeMap<String, LoadedFile>,
    new: BTreeMap<String, LoadedFile>,
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
        let num_left = self.old.len();
        let num_right = self.new.len();

        let keys = self.new.keys().cloned().collect::<Vec<_>>();
        for (idx, f) in keys.iter().enumerate() {
            println!("----- {:?} {}/{} -----", f, idx, num_right);

            let changed_before = self.modified_symbols.len();
            self.load_file(f)?;
            let changed_after = self.modified_symbols.len();

            if changed_after > changed_before {
                println!("❌ -> {}", changed_after - changed_before);
            }
        }

        // for (child, parents) in self.parents.iter() {
        //     self.parents
        //         .entry(child.to_string())
        //         .or_default()
        //         .extend(parents.iter().cloned());
        // }

        // for (parent, children) in self.deps.iter() {
        //     for child in children {
        //         self.parents
        //             .entry(child.to_string())
        //             .or_default()
        //             .insert(parent.to_string());
        //     }
        // }

        // let s = self.modified_symbols.iter().sorted().collect::<Vec<_>>();
        // println!("sorted: {:#?}", s);

        // println!("modified: {:#?}", self.modified_files);

        // for changed in self.modified_symbols.iter() {
        //     self.print_parent(changed);
        // }

        // print the call graph from "_main"
        // self.print_call_graph("_main", 0);

        Ok(())
    }

    fn print_parent(&self, name: &str) {
        println!("\n---- parents: {name} ----");

        let mut stack = vec![];
        for p in self.parents.get(name).unwrap() {
            stack.push((p.to_string(), 0));
        }
        let mut seen = HashSet::new();

        while let Some((current_name, idx)) = stack.pop() {
            // unnamed symbols are fine, but ltmp symbols are garbage
            if current_name.starts_with("ltmp") {
                continue;
            }

            if seen.insert(current_name.clone()) {
                for _ in 0..idx {
                    print!(" ");
                }

                let prs = self.parents.get(&current_name);
                let has_parents = prs.map(|p| !p.is_empty()).unwrap_or(false);

                println!(
                    " {idx} - {} {}",
                    current_name,
                    if has_parents { "" } else { "❌" }
                );

                if let Some(parents) = prs {
                    for parent in parents {
                        stack.push((parent.to_string(), idx + 1));
                    }
                }
            } else {
                // for _ in 0..idx {
                //     print!(" ");
                // }
                // println!("❌ Loop detected -> {current_name}");
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

    fn load_file(&mut self, name: &str) -> Result<()> {
        let new = &self.new[name];
        let Some(old) = self.old.get(name) else {
            println!("no left for {:?}", new.path);
            self.modified_files.entry(new.path.clone()).or_default();
            return Ok(());
        };

        let mut changed = HashSet::new();
        for section in new.macho.sections() {
            let n = section.name().unwrap();
            if n == "__text" || n == "__const" || n.starts_with("__literal") || n == "__eh_frame" {
                let _changed = self.acc_changed(&old.macho, &new.macho, section.index());
                changed.extend(_changed);
            }
        }

        for c in changed.iter() {
            if !c.starts_with("l") && !c.starts_with("ltmp") {
                self.modified_symbols.insert(c.to_string());
            } else {
                let mod_name = format!("{c}_{name}");
                self.modified_symbols.insert(mod_name);
            }
        }

        for (child, parents) in new.parents.iter() {
            let child_name = if child.starts_with("l") {
                format!("{child}_{name}")
            } else {
                child.to_string()
            };

            for p in parents {
                let p_name = if p.starts_with("l") {
                    format!("{p}_{name}")
                } else {
                    p.to_string()
                };

                self.parents
                    .entry(child_name.clone())
                    .or_default()
                    .insert(p_name);
            }
        }

        Ok(())
    }

    fn acc_changed(
        &self,
        old: &'static MachOFile<'_, MachHeader64<Endianness>>,
        new: &'static MachOFile<'_, MachHeader64<Endianness>>,
        section_idx: SectionIndex,
    ) -> HashSet<&'static str> {
        let mut local_modified = HashSet::new();

        // Accumulate modified symbols using masking in functions
        let relocated_new = acc_symbols(&new, section_idx);
        let mut relocated_old = acc_symbols(&old, section_idx)
            .into_iter()
            .map(|f| (f.name, f))
            .collect::<HashMap<_, _>>();

        for right in relocated_new {
            // temp assert while in dev
            let Some(left) = relocated_old.remove(right.name) else {
                local_modified.insert(right.name);
                continue;
            };

            // If the contents of the assembly changed, track it
            if !compare_masked(old, new, &left, &right) {
                // println!(
                //     "Sym [{} ] - {:?}",
                //     right.sym.address(),
                //     // if is_exported { "export" } else { "local" },
                //     right.sym.name().unwrap(),
                //     // pretty_hex::config_hex(
                //     //     &right.data,
                //     //     HexConfig {
                //     //         display_offset: right.sym.address() as usize,
                //     //         ..Default::default()
                //     //     },
                //     // )
                // );
                // println!("❌ Symbols do not match");
                // println!();

                // names might be different, insert both
                local_modified.insert(left.name);
                local_modified.insert(right.name);
            }
        }

        local_modified
    }
}

/// A file loaded into memory with its analysis
///
/// We leak the module to make it easier to deal with its contents
struct LoadedFile {
    path: PathBuf,
    open_file: std::fs::File,
    mmap: &'static Mmap,

    macho: &'static MachOFile<'static, MachHeader64<Endianness>>,

    // name -> symbol
    sym_tab: HashMap<&'static str, RelocatedSymbol<'static>>,

    // symbol -> symbols
    deps: HashMap<&'static str, HashSet<&'static str>>,

    // symbol -> symbols
    parents: HashMap<&'static str, HashSet<&'static str>>,
}

impl LoadedFile {
    fn from_dir(dir: &Path) -> anyhow::Result<BTreeMap<String, Self>> {
        let dir = std::fs::read_dir(dir)?;
        let mut out = BTreeMap::new();
        for f in dir.flatten() {
            let p = f.path();
            if p.extension() != Some(OsStr::new("o")) {
                continue;
            }
            out.insert(
                p.file_name().unwrap().to_string_lossy().to_string(),
                Self::new(p)?,
            );
        }

        Ok(out)
    }

    fn new(path: PathBuf) -> anyhow::Result<Self> {
        let file = std::fs::File::open(&path)?;
        let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };
        let mmap: &'static Mmap = Box::leak(Box::new(mmap));
        let f = File::parse(mmap.deref() as &[u8])?;
        let File::MachO64(macho) = f else { panic!() };
        let macho = Box::leak(Box::new(macho));

        let mut loaded_file = Self {
            path,
            open_file: file,
            mmap,
            macho,
            deps: Default::default(),
            parents: Default::default(),
            sym_tab: Default::default(),
        };

        loaded_file.load();

        Ok(loaded_file)
    }

    fn load(&mut self) {
        // Build the symbol table
        for sect in self.macho.sections() {
            for r in acc_symbols(&self.macho, sect.index()) {
                let existed = self.sym_tab.insert(r.name, r);
                assert!(existed.is_none());
            }
        }

        // Create a map of address -> symbol so we can resolve the section of a symbol
        let local_defs = self
            .macho
            .symbols()
            .filter(|s| s.is_definition())
            .map(|s| (s.address(), s.name().unwrap()))
            .collect::<BTreeMap<_, _>>();

        // Build the call graph by walking the relocations
        for (sym_name, sym) in self.sym_tab.iter() {
            let entry = self.deps.entry(sym_name).or_default();
            let sym_section = self.macho.section_by_index(sym.section).unwrap();
            let sym_data = sym_section.data().unwrap();

            for (addr, reloc) in sym.relocations.iter() {
                let target = match symbol_name_of_relo(self.macho, reloc.target()) {
                    Some(name) => name,
                    None => {
                        let value_bytes = &sym_data[*addr as usize..(*addr + 8) as usize];
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
                        local_defs.get(&addend).unwrap()
                    }
                };

                if target == "__ZN100_$LT$dioxus_core..any_props..VProps$LT$F$C$P$C$M$GT$$u20$as$u20$dioxus_core..any_props..AnyProps$GT$9duplicate17h57c076f81807f7c3E" {
                    println!("{} targetted in {:?}?", sym_name, self.path);
                }
                // if target.starts_with("l") {
                //     println!("target: {target} from {sym_name}");
                // }

                // if target == *sym_name {
                //     println!("Reference to self: {sym_name} - reloc: {reloc:?}");
                // }

                entry.insert(target);
            }
        }

        // Build the parent graph
        for (parent, children) in self.deps.iter() {
            for child in children {
                self.parents.entry(child).or_default().insert(parent);
            }
        }
    }

    fn acc_public_parent_chain<'a>(&self, name: &'a str) -> Vec<&'a str> {
        let mut roots = vec![];

        let mut stack = vec![(name, 0)];
        let mut seen = HashSet::new();

        while let Some((current_name, idx)) = stack.pop() {
            if !seen.insert(current_name) {
                continue;
            }

            let parents = self.parents.get(current_name);

            if !current_name.starts_with("l") && current_name != name {
                roots.push(current_name);
            }

            // let Some(entry) = self.sym_tab.get(current_name) else {
            //     continue;
            // };

            let is_global = self
                .sym_tab
                .get(current_name)
                .map(|s| s.sym.is_global())
                .unwrap_or(true);

            if is_global {
                // roots.push(current_name);
                // println!("global: {current_name}");
            } else {
                // println!("local: {current_name}");
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

type FileRef<'data> = &'data MachOFile<'data, MachHeader64<Endianness>>;

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
        let data = match reloc_start {
            Some(_start) => &data[sym_offset as usize..func_end],
            _ => &[],
        };

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

        func_end = sym_offset as usize;
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
            return false;
        }

        // Check the data
        // the slice is the end of the relocation to the start of the previous relocation
        let reloc_byte_size = (left_reloc.size() as usize) / 8;
        let start = *l_addr as usize - left.offset as usize + reloc_byte_size;

        // Some relocations target the same location
        // In these cases, we just continue since we just masked and checked them already
        if (*l_addr as usize - left.offset as usize) == last {
            continue;
        }

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
            panic!("Absolute relocation target");
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

/// Builds an object file that satisfies the imports
///
/// Creates stub functions that jump to known addresses in a target process.
///
/// .section __TEXT,__text
/// .globl __ZN4core3fmt3num52_$LT$impl$u20$core..fmt..Debug$u20$for$u20$usize$GT$3fmt17h4e710f94be547818E
/// .p2align 2
/// __ZN4core3fmt3num52_$LT$impl$u20$core..fmt..Debug$u20$for$u20$usize$GT$3fmt17h4e710f94be547818E:
///     // Load 64-bit address using immediate values
///     movz x9, #0xCDEF          // Bottom 16 bits
///     movk x9, #0x89AB, lsl #16 // Next 16 bits
///     movk x9, #0x4567, lsl #32 // Next 16 bits
///     movk x9, #0x0123, lsl #48 // Top 16 bits
///
///     // Branch to the loaded address
///     br x9
fn build_stub(adrp_imports: HashMap<&str, u64>) -> Result<Vec<u8>> {
    use object::{
        write::{Object, Section, Symbol, SymbolSection},
        BinaryFormat, Endianness, SectionKind, SymbolFlags, SymbolKind, SymbolScope,
    };
    // Create a new ARM64 object file
    let mut obj = Object::new(
        BinaryFormat::MachO,
        object::Architecture::Aarch64,
        Endianness::Little,
    );

    // Add a text section for our trampolines
    let text_section = obj.add_section(Vec::new(), ".text".into(), SectionKind::Text);

    // For each symbol, create a trampoline that loads the immediate address and jumps to it
    for (name, addr) in adrp_imports {
        let mut trampoline = Vec::new();

        // Break down the 64-bit address into 16-bit chunks
        let addr0 = (addr & 0xFFFF) as u16; // Bits 0-15
        let addr1 = ((addr >> 16) & 0xFFFF) as u16; // Bits 16-31
        let addr2 = ((addr >> 32) & 0xFFFF) as u16; // Bits 32-47
        let addr3 = ((addr >> 48) & 0xFFFF) as u16; // Bits 48-63

        // MOVZ x9, #addr0
        let movz = 0xD2800009 | ((addr0 as u32) << 5);
        trampoline.extend_from_slice(&movz.to_le_bytes());

        // MOVK x9, #addr1, LSL #16
        let movk1 = 0xF2A00009 | ((addr1 as u32) << 5);
        trampoline.extend_from_slice(&movk1.to_le_bytes());

        // MOVK x9, #addr2, LSL #32
        let movk2 = 0xF2C00009 | ((addr2 as u32) << 5);
        trampoline.extend_from_slice(&movk2.to_le_bytes());

        // MOVK x9, #addr3, LSL #48
        let movk3 = 0xF2E00009 | ((addr3 as u32) << 5);
        trampoline.extend_from_slice(&movk3.to_le_bytes());

        // BR x9 - Branch to the address in x9
        let br: u32 = 0xD61F0120;
        trampoline.extend_from_slice(&br.to_le_bytes());

        // Add the trampoline to the text section
        let symbol_offset = obj.append_section_data(text_section, &trampoline, 4);

        // we are writing this:
        // __ZN93_$LT$generational_box..references..GenerationalRef$LT$R$GT$$u20$as$u20$core..fmt..Display$GT$3fmt17h455abb35572b9c11E
        //
        // but we should be writing this:
        //       _$LT$generational_box..references..GenerationalRef$LT$R$GT$$u20$as$u20$core..fmt..Display$GT$::fmt::h455abb35572b9c11
        // let name = strip_mangled(name);

        // // let name = name.trim_start_matches("_");
        // println!("name: {name}");
        let name = if name.starts_with("_") {
            &name[1..]
        } else {
            name
        };

        // Add the symbol
        obj.add_symbol(Symbol {
            name: name.into(),
            value: symbol_offset,
            size: trampoline.len() as u64,
            kind: SymbolKind::Text,
            scope: SymbolScope::Dynamic,
            weak: false,
            section: SymbolSection::Section(text_section),
            flags: SymbolFlags::None,
        });
    }

    obj.write().context("Failed to write object file")
}

fn strip_mangled(name: &str) -> &str {
    if !name.starts_with("__ZN") {
        return name;
    }

    let shorter = name.trim_start_matches("__ZN").trim_end_matches("E");

    // pluck off the leading numbers
    let mut start = 0;
    for c in shorter.chars() {
        if !c.is_numeric() {
            break;
        }
        start += 1;
    }

    &shorter[start..]
}

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

#[test]
fn graph_makes_sense() {
    // "__ZN100_$LT$dioxus_core..any_props..VProps$LT$F$C$P$C$M$GT$$u20$as$u20$dioxus_core..any_props..AnyProps$GT$9duplicate17h57c076f81807f7c3E"
    let f=  "/Users/jonkelley/Development/Tinkering/ipbp/data/incremental-new/harness-ce721ccd4e3f382d.bazp83619w16q5x62o34kz5zp.rcgu.o";
    // let f=  "/Users/jonkelley/Development/Tinkering/ipbp/data/incremental-new/harness-ce721ccd4e3f382d.bazp83619w16q5x62o34kz5zp.rcgu.o";
    // let f=  "/Users/jonkelley/Development/Tinkering/ipbp/data/incremental-new/harness-ce721ccd4e3f382d.bazp83619w16q5x62o34kz5zp.rcgu.o";
    let f = PathBuf::from(f);
    let inpu = LoadedFile::new(f).unwrap();
    let exports = inpu
        .macho
        .exports()
        .unwrap()
        .iter()
        .map(|e| e.name().to_utf8())
        .collect::<Vec<_>>();

    for import in inpu.macho.imports().unwrap() {
        let imp = import.name().to_utf8();

        // this won't acc anything since the symbols haven't been discovered
        let parents = inpu.acc_public_parent_chain(imp);
        println!("imp: {imp:?}");
        for p in parents.iter() {
            println!(
                " -> {p:?} {}",
                if exports.contains(&p) { "✅" } else { "❌" }
            );
        }
        if parents.is_empty() {
            println!(" -> None ❌");
        }

        // println!("imp: {imp:?} - parents: {:#?}", parents);
    }

    // let dep = inpu.sym_tab.get("ltmp2").unwrap();
    // let ps = inpu.parents.get("ltmp2").unwrap();
    // println!("ltmp2: {:#?}", ps);
    // // println!("ltmp2: {:#?}", dep.relocations);
}
