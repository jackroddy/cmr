use cmr::Format;

const MARKDOWN: &str = r"## this is a header

this is a paragraph with **bold** and *italic* text, wrapped at forty columns

- this is a list
- with another item";

fn main() {
    let format = Format::builder().width(40).build();
    let document = cmr::parse(MARKDOWN);
    let page = document.render_with(format);
    println!("{page}");
}
