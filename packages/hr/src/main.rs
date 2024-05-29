// use object::Object;
// use std::io::Read;

use object::{
    macho, Object, ObjectSection, ObjectSymbol, ReadRef, RelocationTarget, Section, SectionIndex,
    SectionKind, SymbolSection,
};
use object::{
    read::macho::{LoadCommandVariant, MachHeader, Nlist},
    Segment,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};
use std::{error::Error, hash::Hash};

fn main() {
    let mut old = None;
    let mut count = 0;

    loop {
        some_func_1(count);
        count += 1;

        std::thread::sleep(std::time::Duration::from_secs(1));
        patch_proc(&mut old);
    }
}

#[no_mangle]
#[inline(never)]
pub fn some_func_1(ct: usize) {
    println!("some_func_0 {ct}");
}

type ObjFiles = HashMap<String, Vec<u8>>;

fn patch_proc(old: &mut Option<ObjFiles>) {
    if old.is_none() {
        *old = Some(collect_obj_files());
        return;
    }

    let new = collect_obj_files();
    let old = old.as_mut().unwrap();

    if old == &new {
        println!("No change in obj files");
        return;
    }

    println!("Change in obj files - apply patches to the running process");

    // for now just write both to the filesystem so we can use them for harnessing
    for (name, bytes) in new.iter() {
        let path = Path::new("./saved/new").join(name);
        fs::write(path, bytes).unwrap();
    }

    for (name, bytes) in old.iter() {
        let path = Path::new("./saved/old").join(name);
        fs::write(path, bytes).unwrap();
    }

    *old = new;
}

fn collect_obj_files() -> ObjFiles {
    let root = std::path::Path::new("/Users/jonkelley/Development/Tinkering/ipbp/");
    let incr_root = root.join("target/aarch64-apple-darwin/debug/incremental/");

    let incremental_search_dirs = fs::read_dir(&incr_root).unwrap();

    let mut files = HashMap::new();

    // find the first folder that has a "hr" prefix and has a folder that contains the .o files
    for dir in incremental_search_dirs {
        let entry = dir.unwrap();

        if entry.file_name().to_str().unwrap().starts_with("hr") {
            for folder in fs::read_dir(entry.path()).unwrap() {
                let folder = folder.unwrap();
                let path = folder.path();

                if !path.is_dir() {
                    continue;
                }

                // push any .os found
                for entry in fs::read_dir(path).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();

                    if !path.file_name().unwrap().to_str().unwrap().ends_with(".o") {
                        continue;
                    }

                    let bytes = fs::read(&path).unwrap();
                    files.insert(
                        path.file_name().unwrap().to_str().unwrap().to_string(),
                        bytes,
                    );
                }

                if !files.is_empty() {
                    break;
                }
            }
        }
    }

    assert!(files.len() > 0, "No object files found");

    files
}

fn apply_patches(old: &ObjFiles, new: &ObjFiles) {
    // just print the keys difference for now
    let old_keys = old.keys().collect::<HashSet<_>>();
    let new_keys = new.keys().collect::<HashSet<_>>();

    let diff = old_keys.symmetric_difference(&new_keys);
    dbg!(diff);
}

#[test]
fn parse_the_various_bins() {
    // Load the binaries

    //
    let root = std::path::Path::new("/Users/jonkelley/Development/Tinkering/ipbp/");
    let incr_root = root.join("target/aarch64-apple-darwin/debug/incremental/");
    let incr_dir =
        incr_root.join("harness-1nd7jxdaw4r01/s-gwm41cynk0-1tvzk8g-403ip6yiub4yfub9kmibhx413");

    dbg!(incr_dir.as_path());
    let incr_dir = incr_dir.canonicalize().unwrap();

    for entry in fs::read_dir(incr_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        // dbg!(&path);

        if !entry
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with(".o")
        {
            continue;
        }

        let bytes = fs::read(&path).unwrap();
        let as_ref = bytes.as_ref() as &[u8];
        let filed = object::read::File::parse(as_ref).unwrap();

        // filed.sections().collect::<Vec<_>>()

        println!("-------------------");
        if filed.symbol_by_name("some_func_1").is_some() {
            println!("\n\nFound some_func_1 in {:?}\n\n", path);
        }

        for section in filed.sections() {
            // dbg!(section.kind());

            let relocs = section.relocations();

            let relocs = relocs.collect::<Vec<_>>();

            if !relocs.is_empty()
                && !matches!(section.kind(), SectionKind::Debug | SectionKind::Unknown)
            {
                println!(
                    "Found {} relocs for section {:?} {:?}",
                    relocs.len(),
                    section.name(),
                    section.kind()
                );

                match section.kind() {
                    SectionKind::Unknown => {}
                    SectionKind::Text => {
                        dbg!(relocs);
                    }
                    SectionKind::Data => {}
                    SectionKind::ReadOnlyData => {}
                    SectionKind::ReadOnlyDataWithRel => {
                        dbg!(relocs);
                    }
                    SectionKind::ReadOnlyString => {}
                    SectionKind::UninitializedData => {}
                    SectionKind::Common => {}
                    SectionKind::Tls => {}
                    SectionKind::UninitializedTls => {}
                    SectionKind::TlsVariables => {}
                    SectionKind::OtherString => {}
                    SectionKind::Other => {}
                    SectionKind::Debug => {}
                    SectionKind::Linker => {}
                    SectionKind::Note => {}
                    SectionKind::Metadata => {}
                    SectionKind::Elf(_) => {}
                    _ => {}
                }
            }

            // match section.kind() {
            //     SectionKind::Unknown => dbg!("Unknown"),
            //     SectionKind::Text => dbg!("Text"),
            //     SectionKind::Data => dbg!("Data"),
            //     SectionKind::ReadOnlyData => dbg!("ReadOnlyData"),
            //     SectionKind::ReadOnlyDataWithRel => dbg!("ReadOnlyDataWithRel"),
            //     SectionKind::ReadOnlyString => dbg!("ReadOnlyString"),
            //     SectionKind::UninitializedData => dbg!("UninitializedData"),
            //     SectionKind::Common => dbg!("Common"),
            //     SectionKind::Tls => dbg!("Tls"),
            //     SectionKind::UninitializedTls => {
            //         //
            //         println!("hey we found a tls!\n {:?}", section);
            //         ""
            //     }
            //     SectionKind::TlsVariables => dbg!("TlsVariables"),
            //     SectionKind::OtherString => dbg!("OtherString"),
            //     SectionKind::Other => dbg!("Other"),
            //     SectionKind::Debug => dbg!("Debug"),
            //     SectionKind::Linker => dbg!("Linker"),
            //     SectionKind::Note => dbg!("Note"),
            //     SectionKind::Metadata => dbg!("Metadata"),
            //     SectionKind::Elf(_) => dbg!("Elf"),
            //     s => dbg!("unknown!"),
            // };
        }

        // dbg!();

        // let s = filed
        //     .sections()
        //     .map(|section| section.name())
        //     .collect::<Vec<_>>();

        // dbg!(s);
    }

    // let bytes = include_bytes!("../../../target/aarch64-apple-darwin/debug/incremental/harness-1nd7jxdaw4r01/s-gwm401zbyv-1cum1xi-403ip6yiub4yfub9kmibhx413/");
    // let _bytes = &*bytes as &[u8];

    // let obj = object::File::parse(_bytes).unwrap();

    // dbg!(obj.sections().collect::<Vec<_>>());
    // dbg!(obj.symbols().collect::<Vec<_>>());

    // let s = obj.dynamic_ations().unwrap();

    // dbg!(s);

    // let bytes = include_bytes!("../../../target/aarch64-apple-darwin/debug/incremental/harness-1nd7jxdaw4r01/s-gwm2qkkvly-td3bqi-7nv98y6kjmqrdtb2wv8q3nyfu/dep-graph.bin");

    // let as_str = std::string::String::from_utf8_lossy(bytes);
    // println!("{:x?}", as_str);
    // let as_value: Option<String> = bincode::deserialize(bytes).unwrap();
}

fn what() -> Result<(), Box<dyn Error>> {
    let path = "./target/aarch64-apple-darwin/debug/harness";
    let data = std::fs::read(path).unwrap();
    // let as_ref = data.as_ref() as &[u8];
    // let filed = object::read::File::parse(as_ref).unwrap();

    let header = macho::MachHeader64::<object::Endianness>::parse(&*data, 0).unwrap();
    let endian = header.endian().unwrap();
    let mut commands = header.load_commands(endian, &*data, 0).unwrap();

    while let Some(command) = commands.next()? {
        let segment = command.variant().unwrap();

        // segment.sections();

        match segment {
            LoadCommandVariant::Segment32(_, _) => {}
            LoadCommandVariant::Symtab(_) => {}
            LoadCommandVariant::Thread(_, _) => {}
            LoadCommandVariant::Dysymtab(_) => {}
            LoadCommandVariant::Dylib(_) => {}
            LoadCommandVariant::IdDylib(_) => {}
            LoadCommandVariant::LoadDylinker(_) => {}
            LoadCommandVariant::IdDylinker(_) => {}
            LoadCommandVariant::PreboundDylib(_) => {}
            LoadCommandVariant::Routines32(_) => {}
            LoadCommandVariant::SubFramework(_) => {}
            LoadCommandVariant::SubUmbrella(_) => {}
            LoadCommandVariant::SubClient(_) => {}
            LoadCommandVariant::SubLibrary(_) => {}
            LoadCommandVariant::TwolevelHints(_) => {}
            LoadCommandVariant::PrebindCksum(_) => {}
            LoadCommandVariant::Segment64(_, _) => {}
            LoadCommandVariant::Routines64(_) => {}
            LoadCommandVariant::Uuid(_) => {}
            LoadCommandVariant::Rpath(_) => {}
            LoadCommandVariant::LinkeditData(_) => {}
            LoadCommandVariant::EncryptionInfo32(_) => {}
            LoadCommandVariant::DyldInfo(_) => {}
            LoadCommandVariant::VersionMin(_) => {}
            LoadCommandVariant::DyldEnvironment(_) => {}
            LoadCommandVariant::EntryPoint(_) => {}
            LoadCommandVariant::SourceVersion(_) => {}
            LoadCommandVariant::EncryptionInfo64(_) => {}
            LoadCommandVariant::LinkerOption(_) => {}
            LoadCommandVariant::Note(_) => {}
            LoadCommandVariant::BuildVersion(_) => {}
            LoadCommandVariant::FilesetEntry(_) => {}
            LoadCommandVariant::Other => {}
            _ => {}
        }

        // if let Some(symtab_command) = command.symtab()? {
        //     let symbols = symtab_command.symbols::<macho::MachHeader64<_>, _>(endian, &*data)?;
        //     for symbol in symbols.iter() {
        //         let name = symbol.name(endian, symbols.strings())?;
        //         let n = String::from_utf8_lossy(name);
        //         // println!("{}", n);

        //         if n.contains("some_func_1") {
        //             println!("{}", n);
        //             // panic!()
        //             // let nlist = Nlist::parse(symbol, endian);
        //             // dbg!(nlist);
        //         }
        //     }
        // }
    }

    Ok(())

    // let s = filed
    //     .sections()
    //     .map(|section| {
    //         // let name = section.name().unwrap();
    //         // let s = section.ations().unwrap();
    //         // (name, s)
    //     })
    //     .collect::<Vec<_>>();

    // dbg!(s);
}

/// https://lief.re/doc/stable/tutorials/11_macho_modification.html
#[test]
fn analyze_diff_between_obj_dumps() {
    static old: include_dir::Dir = include_dir::include_dir!("saved/old");
    static new: include_dir::Dir = include_dir::include_dir!("saved/new");

    let old_keys = old
        .files()
        .map(|f| f.path().file_name().unwrap().to_str().unwrap());

    let new_keys = new
        .files()
        .map(|f| f.path().file_name().unwrap().to_str().unwrap());

    let old_keys = old_keys.collect::<HashSet<_>>();
    let new_keys = new_keys.collect::<HashSet<_>>();

    // nothing changed in the codgen units!
    let diff = old_keys.symmetric_difference(&new_keys);
    dbg!(diff);

    // do we combine all the .o files together?
    // the linker takes a streaming approach, so we actually need the dep graph?
    // could also just operate on codgenunits = 1, but that doesn't solve this in the general case
    // this is because a symbol in A can be used in B, and B can be used in C, etc so the graph itself
    // is the only way to know the full set of symbols that are used in the way that linkers expect
    // we might be able to process each one-by-one

    // figure out which object files changed
    // for each object file, figure out which symbols changed
    // for each symbol, figure out which sections changed
    // for each section, figure out which relocations changed

    for old_o_file in old.files() {
        let fname = old_o_file.path().file_name().unwrap().to_str().unwrap();
        let new_o_file = new.get_file(fname).unwrap();

        // todo: use filesystem stamping or some product from rustc itself
        if old_o_file.contents() == new_o_file.contents() {
            continue;
        }

        println!("Incremental change to {:?}", fname);

        let old_bj = object::read::File::parse(old_o_file.contents() as &[u8]).unwrap();
        let new_bj = object::read::File::parse(new_o_file.contents() as &[u8]).unwrap();

        let old_symbols = old_bj
            .symbols()
            .map(|f| f.name().unwrap())
            .collect::<Vec<_>>();
        dbg!(old_symbols);

        let old_sym = old_bj.symbol_by_name("_some_func_1").unwrap();
        let new_sym = new_bj.symbol_by_name("_some_func_1").unwrap();

        dbg!(&old_sym);
        dbg!(&new_sym);

        dbg!(&old_sym.section());
        // Get the section for this symbol - we want the actual instructions!
        let SymbolSection::Section(id) = old_sym.section() else {
            panic!()
        };

        // these will need to be fixed up against the running process by self-inspection
        let imports = old_bj.imports().unwrap();

        // dbg!(imports);
        for (offset, reloc) in old_bj.section_by_index(id).unwrap().relocations() {
            let target_sym = reloc.target();
            match target_sym {
                RelocationTarget::Symbol(sym) => {
                    let sym = old_bj.symbol_by_index(sym).unwrap();

                    if imports
                        .iter()
                        .any(|f| f.name() == sym.name_bytes().unwrap())
                    {
                        println!("[{offset} import reloc] symbol {:?}", sym);
                    } else {
                        println!("[{offset} normal reloc] {:?}", sym);
                    }
                }

                // currently we don't have any of these types of relocs yet...
                RelocationTarget::Section(section) => {
                    let section = old_bj.section_by_index(section);
                    panic!("Relocing section {:?}", section)
                }
                RelocationTarget::Absolute => {}
                _ => todo!(),
            }
            // println!("Relocing ", old_bj.symbol_by_index(target_sym));

            // dbg!(reloc);
        }

        // dbg!(old_sym.);

        let sction = old_bj.section_by_index(id);
        dbg!(sction.unwrap());
    }

    // for old_sym in
}

#[test]
fn analyze_objs() {
    static old_: include_dir::Dir = include_dir::include_dir!("saved/old");

    for old in old_.entries() {
        let as_file = old.as_file().unwrap();
        let bytes = as_file.contents();

        let as_ref = bytes.as_ref() as &[u8];
        let filed = object::read::File::parse(as_ref).unwrap();

        println!("-------------------");
        let symbols = filed
            .symbols()
            .map(|f| f.name().unwrap())
            .collect::<Vec<_>>();

        if let Some(sym) = symbols.iter().find(|f| f.contains("some_func")) {
            println!("\n\nFound {sym:?} in {:?}\n\n", old.path());
        }

        if filed.symbol_by_name("some_func_1").is_some() {
            panic!()
            // println!("\n\nFound some_func_1 in {:?}\n\n", old.path());
        }

        for section in filed.sections() {
            let relocs = section.relocations().collect::<Vec<_>>();

            if !relocs.is_empty()
                && !matches!(section.kind(), SectionKind::Debug | SectionKind::Unknown)
            {
                println!(
                    "Found {} relocs for section {:?} {:?}",
                    relocs.len(),
                    section.name(),
                    section.kind()
                );

                if matches!(section.kind(), SectionKind::ReadOnlyData)
                    && section.name().unwrap() == "__const"
                {
                    // println!("section {}", section.name().unwrap());

                    // section.data()
                    // let data = section.data().unwrap();
                    // dbg!(data);
                    // let data = data.as_ref();
                    // let data = std::str::from_utf8(&data[..(data.len()64)]).unwrap();

                    // println!("{}", data);
                }

                // // print the contents of this text section
                // let data = section.data().unwrap();
                // let data = data.as_ref();
                // let data = std::str::from_utf8(data).unwrap();

                match section.kind() {
                    SectionKind::Unknown => {}
                    SectionKind::Text => {
                        // dbg!(relocs);
                    }
                    SectionKind::Data => {}
                    SectionKind::ReadOnlyData => {}
                    SectionKind::ReadOnlyDataWithRel => {
                        // dbg!(relocs);
                    }
                    SectionKind::ReadOnlyString => {}
                    SectionKind::UninitializedData => {}
                    SectionKind::Common => {}
                    SectionKind::Tls => {}
                    SectionKind::UninitializedTls => {}
                    SectionKind::TlsVariables => {}
                    SectionKind::OtherString => {}
                    SectionKind::Other => {}
                    SectionKind::Debug => {}
                    SectionKind::Linker => {}
                    SectionKind::Note => {}
                    SectionKind::Metadata => {}
                    SectionKind::Elf(_) => {}
                    _ => {}
                }
            }
        }
    }
}
