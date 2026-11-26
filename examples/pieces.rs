const MARKDOWN: &str = r"## this is a header

this is a paragraph with **bold** and *italic* text in it";

fn main() {
    let page = cmr::parse(MARKDOWN).render();
    for row in page.rows() {
        for (text, style) in &row.pieces {
            let mut codes = Vec::new();
            if style.bold {
                codes.push("1;36");
            }
            if style.italic {
                codes.push("3;31");
            }
            print!("\x1b[{}m{text}\x1b[0m", codes.join(";"));
        }
        println!();
    }
}
