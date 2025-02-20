use std::{
    borrow, env, error,
    ffi::OsStr,
    fs,
    path::{self, PathBuf},
};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::Path,
};

use object::{
    read::macho::Nlist, File, Object, ObjectSection, ObjectSegment, ObjectSymbol,
    ObjectSymbolTable, ReadRef, SectionIndex, SymbolIndex,
};
use pretty_assertions::Comparison;

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
        // println!("relocations: {:#?}", text.relocations().collect::<Vec<_>>());
        // let symbol_names = new_.symbol_map()
        let mut saved_data = text.data().unwrap().to_vec();

        // Walk the symbols in the text section and print the relocations per symbol
        // eventually this will need to include other sections?
        for sym in new_.symbols()
        // .filter(|s| )
        {
            let sect = sym
                .section_index()
                .map(|i| new_.section_by_index(i).unwrap().name());

            // let is_text = sym.section_index() == Some(text.index());
            // if !is_text {
            //     continue;
            // }

            let is_import = !sym.is_definition() && imports.contains(&sym.name().unwrap());

            let is_export = exports.contains_key(&sym.name().unwrap());
            // let is_export = exports.contains_key(&sym.address());

            let name = if is_import {
                "IMPORT"
            } else if is_export {
                "EXPORT"
            } else if sym.is_global() {
                "GLOBAL"
            } else if sym.is_undefined() {
                "UNDEFINED"
            } else {
                match sym.kind() {
                    object::SymbolKind::Unknown => new_
                        .section_by_index(sym.section_index().unwrap())
                        .unwrap()
                        .name()
                        .unwrap(),
                    object::SymbolKind::Text => "Text",
                    object::SymbolKind::Data => "Data",
                    object::SymbolKind::Section => "Section",
                    object::SymbolKind::File => "File",
                    object::SymbolKind::Label => "Label",
                    object::SymbolKind::Tls => "Tls",
                    _ => todo!(),
                }
            };

            println!("Sym [{name}]: {:?}", sym.name().unwrap());
        }

        // for (addr, reloc) in text.relocations() {
        //     let target = reloc.target();
        //     let name = match target {
        //         object::RelocationTarget::Symbol(symbol_index) => {
        //             let symbol = new_.symbol_by_index(symbol_index).unwrap();
        //             symbol.name_bytes().unwrap()
        //         }
        //         object::RelocationTarget::Section(section_index) => {
        //             continue;
        //             // let section = new_.section_by_index(section_index).unwrap();
        //             // section.name_bytes().unwrap()
        //         }
        //         _ => b"absolute",
        //     };

        //     println!(
        //         "{addr:04} {:?} {} implicit: {} -> {}",
        //         reloc.flags(),
        //         reloc.size(),
        //         reloc.has_implicit_addend(),
        //         std::str::from_utf8(name).unwrap()
        //     );
        // }

        // Walk the functions in reverse and figure out the relocations

        // println!("text_length: {text_length}");
        // for e in new_.symbols() {
        //     println!(
        //         "{:?} / {:?} - {} -  {}",
        //         e.name(),
        //         e.section_index()
        //             .map(|f| new_.section_by_index(f).unwrap().name()),
        //         e.address(),
        //         e.is_definition()
        //     );
        // }

        println!()
    }

    Ok(())
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
