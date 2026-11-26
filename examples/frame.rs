use cmr::Format;

const MARKDOWN: &str = r"## this is a header

this is a paragraph with **bold** and *italic* text, wrapped to fit inside the frame

- this is a list
- with another item";

fn main() {
    let format = Format::builder().width(40).build();
    let page = cmr::parse(MARKDOWN).render_with(format);

    println!("┌{}┐", "─".repeat(42));
    for row in page.rows() {
        println!("│ {row} │");
    }
    println!("└{}┘", "─".repeat(42));
}
