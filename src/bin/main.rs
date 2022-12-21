use mft::Iterator as MftIter;
use mft::MftParser;

fn main() {
    let parser = MftParser::from_path("./.test_data/mft.mft").unwrap();
    let iter = MftIter::from(parser);
    for record in iter {
        println!("{:#?}", record);
    }
}
