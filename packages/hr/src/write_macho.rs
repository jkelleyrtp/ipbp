use core::panic;
use std::{collections::HashMap, path::PathBuf};
use std::{io::prelude::Read, process};

use object::{
    write, Object, ObjectComdat, ObjectKind, ObjectSection, ObjectSymbol, RelocationKind,
    RelocationTarget, SectionKind, SymbolFlags, SymbolKind, SymbolSection,
};

/// Strip out the imported symbols from the Mach-O file such that we fool the linker into thinking we've
/// fixed up relocations. We haven't but the linker doesn't know that.
///
/// https://github.com/gimli-rs/object/blob/master/crates/examples/src/objcopy.rs
///
/// We should use builer apis... but they dont exist for macho....
#[test]
pub fn write_simpler_macho() {
    let contents = include_bytes!("../../../saved/arrow/jx7vacigf9h88k6.o");
    let mut in_object = object::read::File::parse(contents as &[u8]).unwrap();

    let mut out_object = object::write::Object::new(
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
        let mut out_symbol = write::Symbol {
            name: in_symbol.name().unwrap_or("").as_bytes().to_vec(),
            value,
            size: in_symbol.size(),
            kind: in_symbol.kind(),
            scope: in_symbol.scope(),
            weak: in_symbol.is_weak(),
            section,
            flags,
        };

        if out_symbol.is_undefined() {
            let sym_name = out_symbol.name().unwrap();
            let demangled = rustc_demangle::demangle(sym_name).to_string();
            println!("undefined symbol: {:?} {:?}", demangled, out_symbol.scope);

            // we want to lie about the symbol being defined by us
            // this is usually due to the stdlib being linked in *or* a symbol existing in another file already
            // if sym_name.starts_with("__ZN") || sym_name.starts_with("_rust") {
            // let's lie and say that we actually are defining the symbol ourselves...
            out_symbol.section = write::SymbolSection::Absolute;
            out_symbol.scope = object::SymbolScope::Dynamic;
            // out_symbol.value = 0xdeadbeef;
            // }
        } else {
            println!("defined symbol: {:?}", out_symbol.name().unwrap());
        }

        let symbol_id = out_object.add_symbol(out_symbol);
        out_symbols.insert(in_symbol.index(), symbol_id);
    }

    for in_section in in_object.sections() {
        if in_section.kind() == SectionKind::Metadata {
            continue;
        }
        let out_section_id = *out_sections.get(&in_section.index()).unwrap();

        for (offset, in_relocation) in in_section.relocations() {
            let symbol = match in_relocation.target() {
                RelocationTarget::Symbol(symbol) => *out_symbols.get(&symbol).unwrap(),
                RelocationTarget::Section(section) => {
                    out_object.section_symbol(*out_sections.get(&section).unwrap())
                }
                _ => panic!("unknown relocation target for {:?}", in_relocation),
            };

            let name = out_object.symbol(symbol).name().unwrap();
            let demangled = rustc_demangle::demangle(name).to_string();

            let skipped = if out_object.symbol(symbol).is_undefined() {
                " [skipped]"
                // " [skipped]"
            } else {
                ""
            };
            // println!(
            //     "relocating{}: {:?} - {:?}",
            //     skipped,
            //     in_relocation.kind(),
            //     demangled,
            // );

            // we want to skip relocations that are against undefined symbols
            // if !out_object.symbol(symbol).is_undefined() {
            //     println!(
            //         "relocating{}: {:?} - {:?}",
            //         skipped,
            //         in_relocation.kind(),
            //         demangled,
            //     );
            //     // continue;
            // } else {
            //     continue;
            // }

            let idx = in_section.index();
            let out_section = out_sections.get_mut(&idx).unwrap();

            // We want to handle absolute relocations by just properly handling them...
            // if in_relocation.kind() == object::RelocationKind::Absolute {
            // get the address of the relocation
            // let addr = in_section.address() + offset;
            // let data = in_section.data().unwrap();
            // let value = data.read(addr as usize).unwrap();
            // let new_value = value + 0xdeadbeef;
            // println!("absolute relocation: {:?} -> {:?}", value, new_value);

            let out_data = out_object.section_mut(*out_section).data_mut();
            let addr = in_section.address();
            let addend = in_relocation.addend();
            let flags = in_relocation.flags();

            println!(
                "len: {}, addr: {addr:x?}, offset: {offset}: section: {out_section:?}, value: {addend:x?}, flags: {flags:?}, known: {:?}, size: {size:?}",
                out_data.len(),
                in_relocation.kind(),
                size = in_relocation.size()
            );

            let (section_address, symbol_address) = match in_relocation.target() {
                RelocationTarget::Symbol(index) => {
                    let symbol = in_object
                        .symbol_by_index(index)
                        .expect("Invalid symbol index");
                    let symbol_address = symbol.address();

                    let section = in_object.section_by_index(in_section.index()).unwrap();

                    (section.address(), symbol_address)
                }
                RelocationTarget::Section(sec) => {
                    let section = in_object.section_by_index(sec).unwrap();
                    let section_address = section.address();
                    (section.address(), section_address)
                }
                RelocationTarget::Absolute => (in_section.address(), offset),
                _ => todo!(),
            };

            // let symbol_name = symbol.name().expect("Failed to get symbol name");
            // Perform the relocation
            // let section_data = section.data().unwrap();

            let relocation_kind = in_relocation.kind();
            let resolved_value = match relocation_kind {
                RelocationKind::Absolute => section_address,
                RelocationKind::Relative => {
                    // Assuming PC-relative relocation
                    let instruction_address = section_address + offset as u64;
                    (symbol_address as i64 - instruction_address as i64) as u64
                }
                _ => continue,
            };

            // let relocation_size = in_relocation.size() / 8;
            // let mut value_bytes = resolved_value.to_le_bytes();
            // section_data[offset..offset + relocation_size]
            //     .copy_from_slice(&value_bytes[..relocation_size]);

            // match in_relocation.size() {
            //     32 => {
            //         // let as_ptr: [u8; 4] = [0xde, 0xad, 0xbe, 0xef];
            //         let resolved_value = resolved_value as u32;
            //         let mut value_bytes = resolved_value.to_le_bytes();
            //         let _offset = offset as usize;
            //         let mut out_data = out_data.as_mut();
            //         let _range = &mut out_data[_offset.._offset + 4];
            //         _range.copy_from_slice(&value_bytes);
            //     }
            //     64 => {
            //         // let as_ptr: [u8; 8] = [0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 0];
            //         let mut value_bytes = resolved_value.to_le_bytes();
            //         let _offset = offset as usize;
            //         let mut out_data = out_data.as_mut();
            //         let _range = &mut out_data[_offset.._offset + 8];
            //         _range.copy_from_slice(&value_bytes);
            //     }
            //     _ => panic!("unknown size: {:?}", in_relocation.size()),
            // }

            // let data = out_object.section_mut(*out_section).data_mut();

            // Apply the relocation to the binary data
            // let relocation_size = in_relocation.size() / 8;
            // let mut value_bytes = resolved_value.to_le_bytes();
            // section_data[offset..offset + relocation_size]
            //     .copy_from_slice(&value_bytes[..relocation_size]);

            // write deadbeef to the relocation - we're gonna come back and fix this up later

            // let as_ptr = 0xdeadbeef as *const u32;
            // let _offset = offset as usize;
            // let mut out_data = out_data.as_mut();
            // let _range = &mut out_data[_offset.._offset + 4];
            // _range.copy_from_slice(unsafe { std::slice::from_raw_parts(as_ptr as *const u8, 4) });
            continue;

            // out_data[] = 0xef;

            // .write_u64(addr as usize, new_value)
            // .unwrap();
            // continue;
            // } else {
            //     println!("Could nto relocate symbol: {:?}", demangled);
            // }

            // let out_relocation = write::Relocation {
            //     offset,
            //     symbol,
            //     addend: in_relocation.addend(),
            //     flags: in_relocation.flags(),
            // };
            // out_object
            //     .add_relocation(out_section_id, out_relocation)
            //     .unwrap();
        }
    }

    for in_comdat in in_object.comdats() {
        let mut sections = Vec::new();
        for in_section in in_comdat.sections() {
            sections.push(*out_sections.get(&in_section).unwrap());
        }
        println!("comdat: {:?}", in_comdat.symbol());
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

    // dbg!(PathBuf::from(".").canonicalize().unwrap());
    let root = PathBuf::from("../../saved/arrow/").canonicalize().unwrap();
    let patched_path = root.join("lib-patch-proc.o");
    let obj = out_object.write().unwrap();
    std::fs::write(&patched_path, obj).unwrap();

    // objdump it
    std::fs::write(
        "../../saved/arrow/dump.txt",
        &std::process::Command::new("objdump")
            .arg("-dr")
            .arg(&patched_path)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();

    // attempt to link it by execing gcc against it
    let output = std::process::Command::new("gcc")
        .arg("-v")
        .arg("-dynamiclib")
        .arg(&patched_path)
        .arg("-o")
        .arg("./saved/arrow/lib-patch.dylib")
        .output()
        .unwrap();

    println!(
        "\n------------ERR-----------\n{}",
        String::from_utf8(output.stderr).unwrap()
    );
}

#[test]
fn print_sections() {
    let contents = include_bytes!("../../../saved/arrow/jx7vacigf9h88k6.o");
    let old_bj = object::read::File::parse(contents as &[u8]).unwrap();

    let sections = old_bj.sections().collect::<Vec<_>>();

    dbg!(old_bj.imports().unwrap());
    dbg!(old_bj.exports().unwrap());

    for section in sections {
        if let Ok(relocs) = section.relocation_map() {
            dbg!(relocs);
        }

        // let relocs = section.relocation_map().unwrap();
    }

    // dbg!();
}
