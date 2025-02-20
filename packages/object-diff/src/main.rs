use std::{
    borrow,
    cmp::Ordering,
    collections::VecDeque,
    env, error,
    ffi::OsStr,
    fs,
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
    ObjectSymbolTable, ReadRef, Relocation, RelocationTarget, SectionIndex, SymbolIndex,
};
use pretty_assertions::Comparison;
use pretty_hex::{Hex, HexConfig};

mod helper;

/// Take a stream of object files and decide which symbols to convert to dynamic lookups
fn main() -> anyhow::Result<()> {
    // Load the object file streams
    let left = read_dir_to_objects(&workspace_dir().join("data").join("incremental-old"));
    let right = read_dir_to_objects(&workspace_dir().join("data").join("incremental-new"));

    // for now, assume cgu puts things into the same files. need to break that assumption eventually
    for right in right.into_iter().nth(12) {
        let Some(left) = left.iter().find(|l| l.file_name() == right.file_name()) else {
            println!("no left for {right:?}");
            continue;
        };

        println!("----- {:?} -----", right.file_name());
        let left_data = fs::read(&left).unwrap();
        let right_data = fs::read(&right).unwrap();

        let File::MachO64(old_) = object::read::File::parse(&left_data as &[u8]).unwrap() else {
            panic!()
        };

        let File::MachO64(new_) = object::read::File::parse(&right_data as &[u8]).unwrap() else {
            panic!()
        };

        let new = Computed::new(&new_);
        let old = Computed::new(&old_);

        let relocated = accumulate_masked_symbols(&new);
        for r in relocated {
            let is_exported = new.exports.contains_key(&r.name);

            println!(
                "Sym [{} - {}] - {:?}\n{}",
                r.sym.address(),
                if is_exported { "export" } else { "local" },
                r.sym.name().unwrap(),
                pretty_hex::config_hex(
                    &r.data,
                    HexConfig {
                        display_offset: r.sym.address() as usize,
                        ..Default::default()
                    },
                )
            );

            for (r_addr, reloc) in r.relocations {
                let (name, kind) = match reloc.target() {
                    object::RelocationTarget::Symbol(symbol_index) => {
                        let symbol = new_.symbol_by_index(symbol_index).unwrap();
                        (symbol.name_bytes().unwrap(), symbol.kind())
                    }
                    object::RelocationTarget::Section(section_index) => {
                        // let section = new_.section_by_index(section_index).unwrap();
                        // section.name_bytes().unwrap()
                        continue;
                    }
                    _ => {
                        b"absolute";
                        continue;
                    }
                };

                // this isn't quite right, I think
                if kind == object::SymbolKind::Data {
                    continue;
                }

                let name = name.to_utf8();
                let is_import = new.imports.contains_key(&name);
                let is_export = new.exports.contains_key(&name);

                println!(
                    "{:04x} [{}] {:?} -> {}",
                    r_addr,
                    if is_import {
                        "imp"
                    } else if is_export {
                        "exp"
                    } else {
                        "loc"
                    },
                    kind,
                    name
                );
            }

            println!()
        }
    }

    Ok(())
}

struct Computed<'data, 'file> {
    file: &'file MachOFile<'data, MachHeader64<Endianness>>,
    exports: HashMap<&'data str, Export<'data>>,
    imports: HashMap<&'data str, Import<'data>>,
    text: MachOSection<'data, 'file, MachHeader64<Endianness>>,
    text_data: &'data [u8],
    text_relocations: Vec<(u64, Relocation)>,
    sorted_functions: Vec<MachOSymbol<'data, 'file, MachHeader64<Endianness>>>,
}

impl<'data, 'file> Computed<'data, 'file> {
    fn new(file: &'file MachOFile<'data, MachHeader64<Endianness>>) -> Self {
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

        // Find our functions - todo - this needs to be done for multiple sections
        let text = file
            .sections()
            .find(|s| s.name_bytes() == Ok(b"__text"))
            .unwrap();

        // Sort them by their address within the text section - necessary so we can determine boundaries for each symbol
        let sorted_functions = file
            .symbols()
            .filter(|s| s.section_index() == Some(text.index()))
            .sorted_by(stable_sort_symbols)
            .collect::<Vec<_>>();

        // Get the relocations for the text section - this will typically be in reverse order
        // We might need to sort these too?
        let text_relocations = text.relocations().collect::<Vec<_>>();

        let text_data = text.data().unwrap();

        Self {
            file,
            exports,
            imports,
            text,
            text_data,
            text_relocations,
            sorted_functions,
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
    offset: usize,
    data: &'a [u8],
    relocations: &'a [(u64, Relocation)],
    sym: &'a MachOSymbol<'a, 'a, MachHeader64<Endianness>>,
}

fn accumulate_masked_symbols<'a, 'b>(new: &'a Computed<'a, 'b>) -> Vec<RelocatedSymbol<'a>> {
    let mut syms = vec![];

    // The end of the currently analyzed function
    let mut func_end = new.text.size() as usize;

    // The idx into the relocation list that applies to this function. We'll march these
    let mut reloc_idx = 0;

    // Walk in reverse so we can use the text_length as the initial backstop and to match relocation order
    for sym in new.sorted_functions.iter().rev() {
        // Only walk the symbols in the text section for now...
        if !(sym.section_index() == Some(new.text.index())) {
            continue;
        }

        // Move the head/tail to include the sub-slice of the relocations that apply to this symbol
        let mut reloc_start = None;
        loop {
            // If we've reached the end of the relocations then we're done
            if reloc_idx == new.text_relocations.len() {
                break;
            }

            // relocations behind the symbol start don't apply
            if new.text_relocations[reloc_idx].0 < sym.address() {
                break;
            }

            // Set the head to the first relocation that applies
            if reloc_start.is_none() {
                reloc_start = Some(reloc_idx);
            }

            reloc_idx += 1;
        }

        // Identify the instructions that apply to this symbol
        let data_range = sym.address() as usize..func_end;
        let data = &new.text_data[data_range.clone()];

        // Identify the relocations that apply to this symbol
        let relocations = match reloc_start {
            Some(start) => &new.text_relocations[start..reloc_idx],
            None => &[],
        };

        syms.push(RelocatedSymbol {
            sym,
            name: sym.name().unwrap(),
            offset: sym.address() as usize,
            data,
            relocations,
        });

        func_end = sym.address() as usize;
    }

    syms
}

/// Compare two sets of bytes, masking out the bytes that are not part of the symbol
/// This is so we can compare functions with different relocations
fn compare_masked<'a>(
    old: &impl Object<'a>,
    new: &impl Object<'a>,
    left: RelocatedSymbol,
    right: RelocatedSymbol,
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
        let left_name = name_of_relocation_target(old, left_target);
        let right_name = name_of_relocation_target(new, right_target);

        // Make sure the names match
        if left_name != right_name {
            println!("names don't match: {left_name:?} != {right_name:?}");
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

struct CachedObjectFile {
    path: PathBuf,
    exports: HashSet<String>,
}

type DepGraph = HashMap<SymbolIndex, HashSet<SymbolIndex>>;

// fn make_function_map()

fn name_of_relocation_target<'a>(obj: &impl Object<'a>, target: RelocationTarget) -> &'a str {
    match target {
        RelocationTarget::Symbol(symbol_index) => {
            let symbol = obj.symbol_by_index(symbol_index).unwrap();
            symbol.name_bytes().unwrap().to_utf8()
        }
        RelocationTarget::Section(section_index) => {
            let section = obj.section_by_index(section_index).unwrap();
            section.name_bytes().unwrap().to_utf8()
        }
        _ => "absolute",
    }
}

fn read_dir_to_objects(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|f| f.path())
        .filter(|p| p.extension() == Some(OsStr::new("o")))
        .collect()
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
