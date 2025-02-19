// use object::{Object, ReadRef};
use object as robject;
use robject::{Object, ReadRef as TreadRef};

// struct DiffResult<'a> {
//     added: Vec<&'a str>,
// }

// pub fn diff<'a, T: Object<'a>>(left: T, right: T) -> DiffResult<'a> {
//     DiffResult { added: vec![] }
// }

fn main() {
    let left = include_bytes!("../data/add-fn-old");
    let right = include_bytes!("../data/add-fn-new");

    let object = robject::read::macho::MachOFile::parse(left).unwrap();
}

// fn read_it<'a, R: TreadRef<'a>>(r: R) {
//     let object = robject::read::macho::MachOFile::parse(r).unwrap();
// }
