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
    ReadRef, SectionIndex, SymbolIndex,
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

            let is_exported = exports.contains_key(&sym.name().unwrap());
            let range = sym.address() as usize..last;
            let data = &text_data[range.clone()];
            let pretty = pretty_hex::config_hex(
                &data,
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

            loop {
                let Some((r_addr, reloc)) = relocations.front() else {
                    break;
                };

                if *r_addr < sym.address() {
                    break;
                }

                let (r_addr, reloc) = relocations.pop_front().unwrap();
                let name = match reloc.target() {
                    object::RelocationTarget::Symbol(symbol_index) => {
                        let symbol = new_.symbol_by_index(symbol_index).unwrap();
                        symbol.name_bytes().unwrap()
                    }
                    object::RelocationTarget::Section(section_index) => {
                        let section = new_.section_by_index(section_index).unwrap();
                        section.name_bytes().unwrap()
                    }
                    _ => b"absolute",
                };
                println!(
                    "{:04x} {:?} {} implicit: {} -> {}",
                    r_addr,
                    reloc.flags(),
                    reloc.size(),
                    reloc.has_implicit_addend(),
                    std::str::from_utf8(name).unwrap()
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

/// Compare two sets of bytes, masking out the bytes that are not part of the symbol
/// This is so we can compare functions with different relocations
fn compare_masked(left: &[u8], right: &[u8], masks: &[Mask]) -> bool {
    todo!()
}

struct CachedObjectFile {
    path: PathBuf,
    exports: HashSet<String>,
}

type DepGraph = HashMap<SymbolIndex, HashSet<SymbolIndex>>;

// fn make_function_map()

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
