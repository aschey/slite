use syntect::dumps::*;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSetBuilder;

fn main() {
    let mut builder = SyntaxSetBuilder::new();
    builder.add_plain_text_syntax();
    builder.add_from_folder("./assets/SQL", true).unwrap();
    let ss = builder.build();
    dump_to_uncompressed_file(&ss, "../../assets/sql.packdump").unwrap();

    let ts = ThemeSet::load_from_folder("./assets/themes").unwrap();
    dump_to_file(&ts, "../../assets/themes.themedump").unwrap();
}
