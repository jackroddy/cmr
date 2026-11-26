const MARKDOWN: &str = r"## this is a header

this is a paragraph with **bold**, *italic* and `inline code` in it

- this is a list
- with another item";

fn main() {
    let document = cmr::parse(MARKDOWN);
    let page = document.render();
    println!("{page}");
}
