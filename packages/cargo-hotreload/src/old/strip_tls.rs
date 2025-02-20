//! Attempt to strip TLS and statics from the object
//!
//! This will force undefined lookups to be resolved at runtime

use core::panic;
use std::{collections::HashMap, io::prelude::Write, path::PathBuf};
use std::{io::prelude::Read, process};

use memmap::MmapOptions;
use object::{
    write, Object, ObjectComdat, ObjectKind, ObjectSection, ObjectSymbol, ReadRef, RelocationKind,
    RelocationTarget, SectionKind, SymbolFlags, SymbolKind, SymbolScope, SymbolSection,
};

#[test]
fn list_symbols() {
    // let contents = include_bytes!("../../../prod_comp/hr_prod_pre-a7e3b91a98f994df.o");
    // let mut in_object = object::read::File::parse(contents as &[u8]).unwrap();

    // for sym in in_object.symbols() {
    //     if sym.kind() == SymbolKind::Data && sym.scope() == SymbolScope::Dynamic {
    //         println!("{:?}", sym);
    //     }
    // }
}

pub fn custom_obj_copy(input: PathBuf, _out: PathBuf) {
    let file = std::fs::File::open(input).unwrap();
    let mmap = unsafe { MmapOptions::new().map(&file).unwrap() };
    let mut in_object = object::read::File::parse(&*mmap).unwrap();

    let out_object = strip_tls(in_object);

    let mut out_file = std::fs::File::create(_out).unwrap();
    out_file.write_all(&out_object).unwrap();
}

pub fn strip_tls(in_object: object::read::File) -> Vec<u8> {
    let mut out_object = write::Object::new(
        in_object.format(),
        in_object.architecture(),
        in_object.endianness(),
    );
    out_object.mangling = write::Mangling::None;
    out_object.flags = in_object.flags();

    let mut out_sections = HashMap::new();
    for in_section in in_object.sections() {
        if in_section.kind() == SectionKind::Metadata {
            continue;
        }
        let section_id = out_object.add_section(
            in_section
                .segment_name()
                .unwrap()
                .unwrap_or("")
                .as_bytes()
                .to_vec(),
            in_section.name().unwrap().as_bytes().to_vec(),
            in_section.kind(),
        );
        let out_section = out_object.section_mut(section_id);
        if out_section.is_bss() {
            out_section.append_bss(in_section.size(), in_section.align());
        } else {
            out_section.set_data(in_section.data().unwrap(), in_section.align());
        }
        out_section.flags = in_section.flags();
        out_sections.insert(in_section.index(), section_id);
    }

    let mut out_symbols = HashMap::new();
    for in_symbol in in_object.symbols() {
        // Strip tls/statics
        if in_symbol.kind() == SymbolKind::Data && in_symbol.scope() == SymbolScope::Dynamic {
            println!("Stripping {:?}", in_symbol);
            continue;
        }

        let (section, value) = match in_symbol.section() {
            SymbolSection::None => (write::SymbolSection::None, in_symbol.address()),
            SymbolSection::Undefined => (write::SymbolSection::Undefined, in_symbol.address()),
            SymbolSection::Absolute => (write::SymbolSection::Absolute, in_symbol.address()),
            SymbolSection::Common => (write::SymbolSection::Common, in_symbol.address()),
            SymbolSection::Section(index) => {
                if let Some(out_section) = out_sections.get(&index) {
                    (
                        write::SymbolSection::Section(*out_section),
                        in_symbol.address() - in_object.section_by_index(index).unwrap().address(),
                    )
                } else {
                    // Ignore symbols for sections that we have skipped.
                    assert_eq!(in_symbol.kind(), SymbolKind::Section);
                    continue;
                }
            }
            _ => panic!("unknown symbol section for {:?}", in_symbol),
        };
        let flags = match in_symbol.flags() {
            SymbolFlags::None => SymbolFlags::None,
            SymbolFlags::Elf { st_info, st_other } => SymbolFlags::Elf { st_info, st_other },
            SymbolFlags::MachO { n_desc } => SymbolFlags::MachO { n_desc },
            SymbolFlags::CoffSection {
                selection,
                associative_section,
            } => {
                let associative_section =
                    associative_section.map(|index| *out_sections.get(&index).unwrap());
                SymbolFlags::CoffSection {
                    selection,
                    associative_section,
                }
            }
            SymbolFlags::Xcoff {
                n_sclass,
                x_smtyp,
                x_smclas,
                containing_csect,
            } => {
                let containing_csect =
                    containing_csect.map(|index| *out_symbols.get(&index).unwrap());
                SymbolFlags::Xcoff {
                    n_sclass,
                    x_smtyp,
                    x_smclas,
                    containing_csect,
                }
            }
            _ => panic!("unknown symbol flags for {:?}", in_symbol),
        };
        let out_symbol = write::Symbol {
            name: in_symbol.name().unwrap_or("").as_bytes().to_vec(),
            value,
            size: in_symbol.size(),
            kind: in_symbol.kind(),
            scope: in_symbol.scope(),
            weak: in_symbol.is_weak(),
            section,
            flags,
        };
        let symbol_id = out_object.add_symbol(out_symbol);
        out_symbols.insert(in_symbol.index(), symbol_id);
    }

    for in_section in in_object.sections() {
        if in_section.kind() == SectionKind::Metadata {
            continue;
        }
        let out_section = *out_sections.get(&in_section.index()).unwrap();
        for (offset, in_relocation) in in_section.relocations() {
            let symbol = match in_relocation.target() {
                RelocationTarget::Symbol(symbol) => *out_symbols.get(&symbol).unwrap(),
                RelocationTarget::Section(section) => {
                    out_object.section_symbol(*out_sections.get(&section).unwrap())
                }
                _ => panic!("unknown relocation target for {:?}", in_relocation),
            };
            let out_relocation = write::Relocation {
                offset,
                symbol,
                addend: in_relocation.addend(),
                flags: in_relocation.flags(),
            };
            out_object
                .add_relocation(out_section, out_relocation)
                .unwrap();
        }
    }

    for in_comdat in in_object.comdats() {
        let mut sections = Vec::new();
        for in_section in in_comdat.sections() {
            sections.push(*out_sections.get(&in_section).unwrap());
        }
        out_object.add_comdat(write::Comdat {
            kind: in_comdat.kind(),
            symbol: *out_symbols.get(&in_comdat.symbol()).unwrap(),
            sections,
        });
    }

    if let Some(in_build_version) = match &in_object {
        object::File::MachO32(file) => file.build_version().unwrap(),
        object::File::MachO64(file) => file.build_version().unwrap(),
        _ => None,
    } {
        let mut out_build_version = object::write::MachOBuildVersion::default();
        out_build_version.platform = in_build_version.platform.get(in_object.endianness());
        out_build_version.minos = in_build_version.minos.get(in_object.endianness());
        out_build_version.sdk = in_build_version.sdk.get(in_object.endianness());
        out_object.set_macho_build_version(out_build_version);
    }

    out_object.write().unwrap()
}
