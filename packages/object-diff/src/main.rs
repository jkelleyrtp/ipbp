use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::{borrow, env, error, fs, path};

use object::{
    read::macho::Nlist, File, Object, ObjectSection, ObjectSegment, ObjectSymbol,
    ObjectSymbolTable, ReadRef, SectionIndex, SymbolIndex,
};

struct DiffResult<'a> {
    added: Vec<&'a str>,
}

pub fn diff<'a, T: Object<'a>>(left: T, right: T) -> DiffResult<'a> {
    DiffResult { added: vec![] }
}

fn main() {
    let left = include_bytes!("../data/add-fn-old");
    let right = include_bytes!("../data/add-fn-new");

    let File::MachO64(old_) = object::read::File::parse(left as &[u8]).unwrap() else {
        panic!()
    };

    let File::MachO64(new_) = object::read::File::parse(right as &[u8]).unwrap() else {
        panic!()
    };

    println!("address: {}", old_.relative_address_base());
    for sect in old_.sections() {
        let data = sect.data().unwrap();
        println!(
            "{:?} [{}] -> {:?}",
            sect.name(),
            data.len(),
            &data[..20.min(data.len())]
        );
    }

    for segment in old_.segments() {
        println!("{:?}", segment.address());
        let data = segment.data().unwrap();
        println!(
            "{:?} [{}] -> {:?} {:?}",
            segment.name(),
            data.len(),
            &data[..20.min(data.len())],
            segment.file_range()
        );
    }

    let addresses = make_section_to_fn_map(&old_);
    let addresses_new = make_section_to_fn_map(&new_);
    let new_map = new_.symbol_map();
    let new_map = new_map
        .symbols()
        .iter()
        .map(|s| (s.name(), s))
        .collect::<HashMap<_, _>>();

    let mut matched = 0;
    let mut mismatched = 0;
    let mut missing = 0;
    for (section_idx, addresses) in addresses {
        let new_addresses = addresses_new.get(&section_idx).unwrap();

        let section = old_.section_by_index(section_idx).unwrap();
        let mut last = section.address() + section.size();
        let data = section.data().unwrap();
        // println!("{:?}", section.name());

        for (addr, symindex) in addresses.iter().rev() {
            let sym = old_.symbol_by_index(*symindex).unwrap();

            if sym.name().unwrap().contains("GCC_except_table") {
                continue;
            }

            let new = new_map.get(sym.name().unwrap());

            let old_instrs =
                &data[(addr - section.address()) as usize..(last - section.address()) as usize];

            if let Some(new) = new {
                let addr = new.address();
                let new_section = new_.section_by_index(section_idx).unwrap();
                let new_data = new_section.data().unwrap();
                let start = (addr - new_section.address()) as usize;
                let new_instrs = &new_data[start..(start + old_instrs.len()).min(new_data.len())];

                if new_instrs != old_instrs {
                    println!("mismatch: {:?}", sym.name());
                    println!("    old: {:02X?}", old_instrs);
                    println!("    new: {:02X?}", new_instrs);
                    matched += 1;
                } else {
                    println!("    okay: {:?}", sym.name());
                    mismatched += 1;
                }
            } else {
                println!("no new symbol for {:?}", sym.name());
                missing += 1;
            }

            // println!("{:?} {:x?}", sym.name(), instrs);

            last = *addr;
        }
    }

    println!("matched: {matched}");
    println!("mismatched: {mismatched}");
    println!("missing: {missing}");

    // for (section_idx, addresses) in addresses {
    //     let new_addresses = addresses_new.get(&section_idx).unwrap();

    //     let section = old_.section_by_index(section_idx).unwrap();
    //     let mut last = section.address() + section.size();
    //     let data = section.data().unwrap();
    //     // println!("{:?}", section.name());

    //     for (addr, symindex) in addresses.iter().rev() {
    //         last = *addr;
    //         let sym = old_.symbol_by_index(*symindex).unwrap();

    //         let new = new_map.get(sym.name().unwrap());

    //         let old_instrs =
    //             &data[(addr - section.address()) as usize..(last - section.address()) as usize];

    //         if let Some(new) = new {
    //             let addr = new.address();
    //             let new_section = new_.section_by_index(section_idx).unwrap();
    //             let new_data = new_section.data().unwrap();
    //             let start = (addr - new_section.address()) as usize;
    //             let new_instrs = &new_data[start..start + old_instrs.len()];

    //             if new_instrs != old_instrs {
    //                 println!("mismatch: {:?}", sym.name());
    //             } else {
    //                 println!("all instrs okay: {:?}", sym.name());
    //             }
    //         } else {
    //             println!("no new symbol for {:?}", sym.name());
    //         }

    //         // println!("{:?} {:x?}", sym.name(), instrs);
    //     }
    // }
}

fn make_section_to_fn_map(
    old: &object::read::macho::MachOFile<'_, object::macho::MachHeader64<object::Endianness>>,
) -> HashMap<SectionIndex, BTreeMap<u64, SymbolIndex>> {
    let mut addresses: HashMap<SectionIndex, BTreeMap<u64, SymbolIndex>> = HashMap::new();

    for symbol in old.symbols() {
        if !symbol.is_definition() {
            continue;
        }

        // If you want to read the function contents
        // if let Some(section) = symbol.section() {
        let section = symbol.section();
        let Some(index) = section.index() else {
            println!("No section for {:?}", symbol.name());
            continue;
        };

        if let Ok(section) = old.section_by_index(index) {
            if let Ok(data) = section.data() {
                if data.len() == 0 {
                    continue;
                }

                if !symbol.is_definition() {
                    continue;
                }

                if section.address() > symbol.address() {
                    println!(
                        "??? {:?} -> {:?} | {:?}, {:?}",
                        symbol.name(),
                        section.name(),
                        symbol.address(),
                        section.address()
                    );
                    continue;
                }

                addresses
                    .entry(section.index())
                    .or_default()
                    .insert(symbol.address(), symbol.index());
            }
        }
    }
    addresses
}

// for sym in old.symbols() {
//     sym.name()
// }

// let table = old.macho_symbol_table();

// for ta in table.iter() {
//     let name = ta.name(old.endian(), table.strings()).unwrap();
//     println!("{:?}", std::str::from_utf8(name));
//     // table.strings().get()
//     // println!("{:?}", ta);
// }

// let const_data = old
//     .segments()
//     .find(|seg| seg.name_bytes().unwrap() == Some(b"__DATA"))
//     .unwrap();

// 4295442432
// 4295444928
// println!("const_data: {:?}", const_data.address());

// println!("const_data: {:?}", const_data.data());

// for sym in old.symbols() {
//     if sym.is_definition() {
//         // if sym.kind() == object::SymbolKind::Data {
//         let scidx = sym.section_index().unwrap();
//         let sect = old.section_by_index(scidx).unwrap();
//         // if sect.name() == Ok("__const") {
//         let data = sect.data().unwrap();
//         // println!(
//         //     "{:?} -> {:?} -> {:?} -> {:?}",
//         //     sym.name().unwrap(),
//         //     sym.address(),
//         //     sym.size(),
//         //     sect.name()
//         // );
//         // }

//         // println!(
//         //     "{:?} -> {:?} -> {:?}",
//         //     sym.name().unwrap(),
//         //     sym.address(),
//         //     sym.section_index()
//         // );
//         // }
//     }
//     // println!("{:?} -> {:?}", sym.name().unwrap(), sym.address());
// }

// let endian = if old.is_little_endian() {
//     gimli::RunTimeEndian::Little
// } else {
//     gimli::RunTimeEndian::Big
// };
// println!("dumping...");
// dump_file(&File::MachO64(old), endian).unwrap();

// let old_syms = old
//     .symbols()
//     .map(|s| s.name().unwrap())
//     .collect::<HashSet<_>>();
// let mut local_symols = vec![];

// let sym_tab = old.symbol_table().unwrap();
// for sym in sym_tab.symbols() {
//     if sym.is_definition() {
//         local_symols.push(sym.index())
//         // println!("{:?} -> {:?}", sym.name().unwrap(), sym.address());
//     }

//     // println!("{:?} -> {:?}", sym.name().unwrap(), sym.scope());
// }

// for idx in local_symols {
//     let sym = old.symbol_by_index(idx).unwrap();
// }

// let new = object::read::File::parse(right as &[u8]).unwrap();
// let new_syms = new
//     .symbols()
//     .map(|s| s.name().unwrap())
//     .collect::<HashSet<_>>();

//     println!("old: {:#?}", old_syms);
//     println!("new: {:#?}", new_syms);

//     println!("New: ");
//     new_syms.difference(&old_syms).for_each(|sym| {
//         println!("new: {}", sym);
//     });

//     println!("Old: ");
//     old_syms.difference(&new_syms).for_each(|sym| {
//         println!("old: {}", sym);
//     });

// for sym in old.symbol_table().unwrap() {}

fn dump_file(
    object: &object::File,
    endian: gimli::RunTimeEndian,
) -> Result<(), Box<dyn error::Error>> {
    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<borrow::Cow<[u8]>, Box<dyn error::Error>> {
        Ok(match object.section_by_name(id.name()) {
            Some(section) => section.uncompressed_data()?,
            None => borrow::Cow::Borrowed(&[]),
        })
    };

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    let borrow_section = |section| gimli::EndianSlice::new(borrow::Cow::as_ref(section), endian);

    // Load all of the sections.
    let dwarf_sections = gimli::DwarfSections::load(&load_section)?;

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf_sections.borrow(borrow_section);

    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        println!(
            "Line number info for unit at <.debug_info+0x{:x}>",
            header.offset().as_debug_info_offset().unwrap().0
        );
        let unit = dwarf.unit(header)?;
        let unit = unit.unit_ref(&dwarf);

        // Get the line program for the compilation unit.
        if let Some(program) = unit.line_program.clone() {
            let comp_dir = if let Some(ref dir) = unit.comp_dir {
                path::PathBuf::from(dir.to_string_lossy().into_owned())
            } else {
                path::PathBuf::new()
            };

            // Iterate over the line program rows.
            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row()? {
                if row.end_sequence() {
                    // End of sequence indicates a possible gap in addresses.
                    println!("{:x} end-sequence", row.address());
                } else {
                    // Determine the path. Real applications should cache this for performance.
                    let mut path = path::PathBuf::new();
                    if let Some(file) = row.file(header) {
                        path.clone_from(&comp_dir);

                        // The directory index 0 is defined to correspond to the compilation unit directory.
                        if file.directory_index() != 0 {
                            if let Some(dir) = file.directory(header) {
                                path.push(unit.attr_string(dir)?.to_string_lossy().as_ref());
                            }
                        }

                        path.push(
                            unit.attr_string(file.path_name())?
                                .to_string_lossy()
                                .as_ref(),
                        );
                    }

                    // Determine line/column. DWARF line/column is never 0, so we use that
                    // but other applications may want to display this differently.
                    let line = match row.line() {
                        Some(line) => line.get(),
                        None => 0,
                    };
                    let column = match row.column() {
                        gimli::ColumnType::LeftEdge => 0,
                        gimli::ColumnType::Column(column) => column.get(),
                    };

                    println!("{:x} {}:{}:{}", row.address(), path.display(), line, column);
                }
            }
        }
    }
    Ok(())
}
