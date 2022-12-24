use mft::Iterator as MftIter;
use mft::{Parser, ParserSettings};

fn main() {
    simple_logger::init_with_env().unwrap();

    let parser = Parser::with_settings(
        "./.test_data/mft.mft",
        ParserSettings::new()
            .drive_char('C')
            .path_exclusion_regex(".*/Windows/.*"),
    )
    .unwrap();

    let iter = MftIter::from(parser);
    for record in iter {
        println!("{}", record);
    }
}
