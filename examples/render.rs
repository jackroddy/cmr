use std::{env, fs};

fn main() {
    let path = env::args().nth(1).expect("usage: render <file.md>");
    let text = fs::read_to_string(path).unwrap();
    println!("{}", cmr::parse(&text).render());
}
