//! try to determine how inter-object symbol patching is expected to be performed
//! This should help prevent us from having to relink everything in a single project, ideally
//! solving issues with statics and other symbols that are not easily patched within a single project

use std::collections::HashSet;

use include_dir::include_dir;
use object::{Object, ObjectSection, ObjectSymbol, RelocationKind, RelocationTarget};

#[test]
fn print_missing_imports() {
    _print_missing_imports();
}

fn _print_missing_imports() {
    static dir: include_dir::Dir = include_dir!("linker_artifacts/saved_objects");

    let mut unresolved = HashSet::new();
    let mut resolved = HashSet::new();

    for item in dir.files() {
        // skip the one causing an issue
        if item.path().to_str().unwrap().contains("31ip6nucdiq1sy7o") {
            continue;
        }

        println!("{:?}", item.path());
        let contents = item.contents();
        let mut in_object = object::read::File::parse(contents as &[u8]).unwrap();

        let imps = in_object.imports().unwrap();
        // for imp in imps {
        //     let name = std::str::from_utf8(imp.name()).unwrap();
        //     // println!("{:?}", name);
        //     unresolved.insert(name);
        // }

        for export in in_object.exports().unwrap() {
            let name = std::str::from_utf8(export.name()).unwrap();
            // println!("{:?}", name);
            resolved.insert(name);
        }

        // print relocations and look for symbols that are unresolved
        for section in in_object.sections() {
            for (offset, relo) in section.relocations() {
                match relo.target() {
                    RelocationTarget::Symbol(sym) => {
                        let located_sym = in_object.symbol_by_index(sym).unwrap();
                        let name = located_sym.name().unwrap();

                        match relo.flags() {
                            object::RelocationFlags::MachO {
                                r_type,
                                r_pcrel,
                                r_length,
                            } => {
                                // GOT entries are decided by the compiler as it generates obj files
                                // if a GOT entry is missing, it's likely the symbol is in another file
                                if r_type == object::macho::X86_64_RELOC_GOT
                                    && relo.kind() == RelocationKind::Unknown
                                    && located_sym.is_undefined()
                                {
                                    println!("found it \n{:#?}\n{:#?}", relo, located_sym);
                                    unresolved.insert(name);
                                }
                            }
                            object::RelocationFlags::Elf { r_type } => {}
                            object::RelocationFlags::Generic {
                                kind,
                                encoding,
                                size,
                            } => todo!(),
                            object::RelocationFlags::Coff { typ } => todo!(),
                            object::RelocationFlags::Xcoff { r_rtype, r_rsize } => todo!(),
                            _ => todo!(),
                        }

                        // if name.contains("h6fd5f23e580bf6b3") {
                        //     panic!("found it \n{:#?}\n{:#?}", relo, located_sym)
                        // }

                        // println!("sym relo {:?}", name);
                        // let name = std::str::from_utf8(located_sym.name().unwrap()).unwrap();

                        // unresolved.insert(name);
                    }
                    RelocationTarget::Section(sect) => {}
                    RelocationTarget::Absolute => {
                        println!("abs relo? {:?}", relo);
                    }
                    _ => {}
                }
                // if relo.target()
                // println!("{:?}", relo);
            }

            // section.relocation_map().unwrap()
            // let section = section.unwrap();
            // if section.relocations().is_ok() {
            //     for relo in section.relocations().unwrap() {
            //         let name = std::str::from_utf8(relo.symbol()).unwrap();
            //         // println!("{:?}", name);
            //         unresolved.insert(name);
            //     }
            // }
        }

        // for relo in in_object.dynamic_relocations().unwrap() {
        // let name = std::str::from_utf8(relo.symbol()).unwrap();
        // // println!("{:?}", name);
        // unresolved.insert(name);
        // }
    }

    // print the unresolved imports
    println!("Unresolved imports:");
    for item in unresolved {
        if !resolved.contains(item) {
            println!("{:?}", item);
        }
    }

    // Load the object file that has the likely missing symbol
    let item = dir
        .files()
        .find(|f| f.path().to_str().unwrap().contains("31ip6nucdiq1sy7o"))
        .unwrap();

    let contents = item.contents();
    let mut in_object = object::read::File::parse(contents as &[u8]).unwrap();
    dbg!(in_object.exports().unwrap());
}
