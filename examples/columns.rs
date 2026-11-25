use cmr::Format;

const TOP: &str = r"# this is a header

this is a paragraph that runs across the full width, above both of the columns";

const LEFT: &str = r"this is a paragraph with **bold** and *italic* text in it

- this is a list
- with another item
  - blah blah blah";

const RIGHT: &str = r"this is a paragraph with `inline code` and [a link](https://example.com)

1. this is a numbered list
2. with a second item
3. and a third";

const BOTTOM: &str = r#"```rust
// this is a code block under both of the columns
let text = "this is a string";
```"#;

fn main() {
    let full = Format::builder().width(63).build();
    let column = Format::builder().width(30).height(6).build();
    let rule = "─".repeat(63);

    println!("{}", cmr::parse(TOP).render_with(full));
    println!("{rule}");
    let left = cmr::parse(LEFT).render_with(column);
    let right = cmr::parse(RIGHT).render_with(column);
    for (l, r) in left.rows().iter().zip(right.rows()) {
        println!("{l} │ {r}");
    }
    println!("{rule}");
    println!("{}", cmr::parse(BOTTOM).render_with(full));
}
