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
    read::macho::{MachOSymbol, Nlist},
    Endianness, File, Object, ObjectSection, ObjectSegment, ObjectSymbol, ObjectSymbolTable,
    ReadRef, Relocation, RelocationTarget, SectionIndex, SymbolIndex,
};
use pretty_assertions::Comparison;
use pretty_hex::{Hex, HexConfig};

mod helper;

/// Take a stream of object files and decide which symbols to convert to dynamic lookups
fn main() -> anyhow::Result<()> {
    // Load the object file streams
    let left = read_dir_to_objects(&workspace_dir().join("data").join("incremental-old"));
    let right = read_dir_to_objects(&workspace_dir().join("data").join("incremental-new"));

    //
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

        let exports = new_
            .exports()
            .unwrap()
            .iter()
            .map(|e| (e.name().to_utf8(), *e))
            // .map(|e| (e.address(), *e))
            .collect::<HashMap<_, _>>();
        let imports = new_
            .imports()
            .unwrap()
            .iter()
            .map(|i| i.name().to_utf8())
            .collect::<HashSet<_>>();
        // let imports = new_.symbol_map().get(address)
        // let imports = new_
        //     .imports()
        //     .unwrap()
        //     .iter()
        //     .map(|i| (i.address(), *i))
        //     .collect();
        // let exports = new_
        //     .exports()
        //     .unwrap()
        //     .iter()
        //     .map(|e| e.name().to_utf8())
        //     .collect::<HashSet<_>>();

        // Find our functions
        let text = new_
            .sections()
            .find(|s| s.name_bytes() == Ok(b"__text"))
            .unwrap();
        let text_length = text.size();
        let text_data = text.data().unwrap();
        let mut relocations = text.relocations().collect::<VecDeque<_>>();
        let mut saved_data = text.data().unwrap().to_vec();

        let sorted_symbols = new_
            .symbols()
            .filter(|s| s.section_index() == Some(text.index()))
            .sorted_by(stable_sort_symbols);

        // Walk the symbols in the text section and print the relocations per symbol
        // eventually this will need to include other sections?
        // We're going backwards so we can use the text_length as the initial backstop
        let mut last = text_length as usize;
        for sym in sorted_symbols.into_iter().rev() {
            // for sym in sorted_symbols.into_iter().rev() {
            // Only walk the symbols in the text section for now...
            if !(sym.section_index() == Some(text.index())) {
                continue;
            }

            let mut cur_relocs = vec![];

            loop {
                let Some((r_addr, reloc)) = relocations.front() else {
                    break;
                };

                if *r_addr < sym.address() {
                    break;
                }

                let (r_addr, reloc) = relocations.pop_front().unwrap();
                cur_relocs.push((r_addr, reloc));
            }

            let is_exported = exports.contains_key(&sym.name().unwrap());
            let range = sym.address() as usize..last;
            let data = &text_data[range.clone()];
            let mut relocated_data = data.to_vec();

            // undo the relocations by writing dumb bytes
            for (r_addr, reloc) in cur_relocs.iter() {
                if reloc.size() == 32 {
                    let r_addr = *r_addr as usize - sym.address() as usize;
                    relocated_data[r_addr as usize..r_addr as usize + 4]
                        .copy_from_slice(&0x70707070_i32.to_be_bytes());
                } else {
                    panic!()
                }
            }

            // Make sure our masking is correct
            let matche = compare_masked(
                &new_,
                &new_,
                ComparedSymbol {
                    offset: sym.address() as usize,
                    data: &data,
                    relocations: &cur_relocs,
                },
                ComparedSymbol {
                    offset: sym.address() as usize,
                    data: &relocated_data,
                    relocations: &cur_relocs,
                },
            );
            assert!(matche);

            let pretty = pretty_hex::config_hex(
                &relocated_data,
                // &data,
                HexConfig {
                    display_offset: sym.address() as usize,
                    ..Default::default()
                },
            );

            println!(
                "Sym [{} - {}] - {:?}\n{}",
                sym.address(),
                if is_exported { "export" } else { "local" },
                sym.name().unwrap(),
                pretty // sym.kind(),
            );

            for (r_addr, reloc) in cur_relocs {
                let (name, kind) = match reloc.target() {
                    object::RelocationTarget::Symbol(symbol_index) => {
                        let symbol = new_.symbol_by_index(symbol_index).unwrap();
                        (symbol.name_bytes().unwrap(), symbol.kind())
                    }
                    object::RelocationTarget::Section(section_index) => {
                        continue;
                        // let section = new_.section_by_index(section_index).unwrap();
                        // section.name_bytes().unwrap()
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
                let is_import = imports.contains(&name);
                let is_export = exports.contains_key(&name);

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

            println!();

            last = sym.address() as usize;
        }

        assert!(relocations.is_empty());

        println!()
    }

    Ok(())
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

// /// A symbol with its paired relocations and data
// struct SymbolWithRelocations<'a> {}

struct Mask<'a> {
    idx: usize,
    bytes: &'a [u8],
}

struct ComparedSymbol<'a> {
    offset: usize,
    data: &'a [u8],
    relocations: &'a [(u64, Relocation)],
}

/// Compare two sets of bytes, masking out the bytes that are not part of the symbol
/// This is so we can compare functions with different relocations
fn compare_masked<'a>(
    old: &impl Object<'a>,
    new: &impl Object<'a>,
    left: ComparedSymbol,
    right: ComparedSymbol,
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
