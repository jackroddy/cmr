//! A small markdown subset, parsed into blocks and laid out as
//! lines of an exact width: headings, paragraphs, lists, fenced
//! code, tables, rules, quotes, bold, inline code and links. Everything else stays
//! literal text.

mod hl;

use hl::{Class, Lexer};

/// One of the sixteen ANSI colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    DarkGray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    White,
}

/// How a piece of text is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bold: bool,
    pub dim: bool,
    pub underline: bool,
    /// The index into the document's links, for link text.
    pub link: Option<usize>,
}

impl Style {
    const fn new() -> Style {
        Style {
            fg: None,
            bold: false,
            dim: false,
            underline: false,
            link: None,
        }
    }

    const fn fg(mut self, color: Color) -> Style {
        self.fg = Some(color);
        self
    }

    const fn bold(mut self) -> Style {
        self.bold = true;
        self
    }

    const fn dim(mut self) -> Style {
        self.dim = true;
        self
    }

    const fn underline(mut self) -> Style {
        self.underline = true;
        self
    }

    fn patch(self, other: Style) -> Style {
        Style {
            fg: other.fg.or(self.fg),
            bold: self.bold || other.bold,
            dim: self.dim || other.dim,
            underline: self.underline || other.underline,
            link: other.link.or(self.link),
        }
    }
}

/// A run of text and the style it is drawn in.
pub type Piece = (String, Style);

/// One line of output, left to right.
pub type Row = Vec<Piece>;

/// How a row follows the row above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// The row starts a line of the text.
    Break,
    /// The row goes on from the one above, which broke at a space.
    Space,
    /// The row goes on from the one above, which broke inside a word.
    Word,
}

/// Where a row's text starts and how the row follows the one above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    /// The columns before the text: a quote's bar, a code block's gutter,
    /// or a list item's indent on the rows after its first.
    pub lead: usize,
    pub join: Join,
}

const BREAK: Mark = Mark {
    lead: 0,
    join: Join::Break,
};

const GUTTERED: Mark = Mark {
    lead: 2,
    join: Join::Break,
};

/// A run of text inside a paragraph, list item or table cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    /// Text between `**` markers, which may hold inline code and links.
    Bold(Vec<Inline>),
    /// Link text, drawn in place of `[text](target)`. The text may hold
    /// bold and inline code.
    Link {
        text: Vec<Inline>,
        /// Where `link` sits in the document's links.
        index: usize,
        link: Link,
    },
}

/// Where a link points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub target: String,
    pub title: Option<String>,
}

/// One entry of a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// How far the item is nested, from its indent.
    pub depth: usize,
    /// The number of a numbered item, `None` for a bullet.
    pub number: Option<u64>,
    pub text: Vec<Inline>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading {
        /// One to three, the number of `#`.
        level: u8,
        text: Vec<Inline>,
    },
    Paragraph(Vec<Inline>),
    List(Vec<Item>),
    Code {
        lang: Option<String>,
        lines: Vec<String>,
        /// Whether a line wider than the block wraps onto
        /// continuation rows, where otherwise it is cut.
        wrap: bool,
        /// The name given by `tab="<name>"` after the language.
        tab: Option<String>,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Rule,
    Quote(Vec<Block>),
}

struct Theme {
    heading: Style,
    header: Style,
    code: Style,
    link: Style,
    lang: Style,
    bullet: Style,
    rule: Style,
    quote: Style,
    comment: Style,
    string: Style,
    number: Style,
    keyword: Style,
}

const THEME: Theme = Theme {
    heading: Style::new().bold(),
    header: Style::new().bold(),
    code: Style::new().fg(Color::Cyan),
    link: Style::new().underline(),
    lang: Style::new().dim(),
    bullet: Style::new().bold(),
    rule: Style::new().dim(),
    quote: Style::new().dim(),
    // inside a code block the hues name token kinds, not the
    // concepts they stand for elsewhere; the gutter keeps them apart
    comment: Style::new().dim(),
    string: Style::new().fg(Color::Green),
    number: Style::new().fg(Color::Magenta),
    keyword: Style::new().fg(Color::Blue),
};

const MIN_COLUMN: usize = 8;

/// `text` parsed and laid out at `width`.
pub fn render(text: &str, width: usize) -> Vec<Row> {
    layout(&parse(text), width)
}

/// The blocks of `text`, read a line at a time.
pub fn parse(text: &str) -> Vec<Block> {
    parse_from(text, &mut 0)
}

/// The links of `blocks` in document order, so that `links[i]` is
/// the link whose `Style::link` is `Some(i)`.
pub fn links(blocks: &[Block]) -> Vec<Link> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            Block::Heading { text, .. } | Block::Paragraph(text) => collect_links(text, &mut out),
            Block::List(items) => items.iter().for_each(|i| collect_links(&i.text, &mut out)),
            Block::Table { header, rows } => {
                for cell in header.iter().chain(rows.iter().flatten()) {
                    collect_links(cell, &mut out);
                }
            }
            Block::Quote(inner) => out.extend(links(inner)),
            Block::Code { .. } | Block::Rule => {}
        }
    }
    out
}

fn collect_links(text: &[Inline], out: &mut Vec<Link>) {
    for inline in text {
        match inline {
            Inline::Bold(inner) => collect_links(inner, out),
            Inline::Link { link, .. } => out.push(link.clone()),
            Inline::Text(_) | Inline::Code(_) => {}
        }
    }
}

/// `parse`, numbering links from `*next`.
fn parse_from(text: &str, next: &mut usize) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut para: Vec<Inline> = Vec::new();
    let mut list: Vec<Item> = Vec::new();
    let mut text_column = 0;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if quoted(line).is_some() {
            flush(&mut blocks, &mut para, &mut list);
            let mut inner = Vec::new();
            while let Some(rest) = lines.get(i).and_then(|l| quoted(l)) {
                inner.push(rest);
                i += 1;
            }
            blocks.push(Block::Quote(parse_from(&inner.join("\n"), next)));
            continue;
        }
        if let Some((ticks, lang, tab)) = fence(line) {
            flush(&mut blocks, &mut para, &mut list);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !closes_fence(lines[i], ticks) {
                code.push(lines[i].replace('\t', "    "));
                i += 1;
            }
            blocks.push(Block::Code {
                lang,
                lines: code,
                wrap: false,
                tab,
            });
            i += 1;
            continue;
        }
        if line.contains('|') && lines.get(i + 1).is_some_and(|l| delimiter_row(l)) {
            flush(&mut blocks, &mut para, &mut list);
            let header = cells(line, next);
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].contains('|') {
                rows.push(cells(lines[i], next));
                i += 1;
            }
            blocks.push(Block::Table { header, rows });
            continue;
        }
        if let Some((level, rest)) = heading(line) {
            flush(&mut blocks, &mut para, &mut list);
            blocks.push(Block::Heading {
                level,
                text: inlines(rest, next),
            });
            i += 1;
            continue;
        }
        if rule(line) {
            flush(&mut blocks, &mut para, &mut list);
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }
        if let Some((item, col)) = list_item(line, next) {
            flush_paragraph(&mut blocks, &mut para);
            // a top-level item of the other kind starts a new list
            let other_kind = item.depth == 0
                && list
                    .first()
                    .is_some_and(|f| f.number.is_some() != item.number.is_some());
            if other_kind {
                flush(&mut blocks, &mut para, &mut list);
            }
            list.push(item);
            text_column = col;
            i += 1;
            continue;
        }
        if line.trim().is_empty() {
            flush_paragraph(&mut blocks, &mut para);
            i += 1;
            continue;
        }
        if let Some(last) = list.last_mut() {
            if indent(line) >= text_column {
                join(&mut last.text, inlines(line.trim(), next));
                i += 1;
                continue;
            }
            flush(&mut blocks, &mut para, &mut list);
        }
        join(&mut para, inlines(line.trim(), next));
        i += 1;
    }
    flush(&mut blocks, &mut para, &mut list);
    blocks
}

fn flush(blocks: &mut Vec<Block>, para: &mut Vec<Inline>, list: &mut Vec<Item>) {
    flush_paragraph(blocks, para);
    if !list.is_empty() {
        blocks.push(Block::List(std::mem::take(list)));
    }
}

fn flush_paragraph(blocks: &mut Vec<Block>, para: &mut Vec<Inline>) {
    if !para.is_empty() {
        blocks.push(Block::Paragraph(std::mem::take(para)));
    }
}

/// Append a line's inlines to `to`, a space between the lines.
fn join(to: &mut Vec<Inline>, line: Vec<Inline>) {
    if !to.is_empty() {
        push_text(to, " ");
    }
    for inline in line {
        match inline {
            Inline::Text(t) => push_text(to, &t),
            other => to.push(other),
        }
    }
}

fn push_text(to: &mut Vec<Inline>, s: &str) {
    if let Some(Inline::Text(t)) = to.last_mut() {
        t.push_str(s);
    } else {
        to.push(Inline::Text(s.to_string()));
    }
}

/// The level and text of a heading line: one to three `#`, a
/// space, and text.
fn heading(line: &str) -> Option<(u8, &str)> {
    let line = line.trim_start();
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=3).contains(&hashes) {
        return None;
    }
    let rest = line[hashes..].strip_prefix(' ')?.trim();
    (!rest.is_empty()).then_some((hashes as u8, rest))
}

/// A quote line without its marker: `> ` or a bare `>`.
fn quoted(line: &str) -> Option<&str> {
    if line == ">" {
        return Some("");
    }
    line.strip_prefix("> ")
}

/// Whether `line` is a rule: three or more `-` and nothing else.
fn rule(line: &str) -> bool {
    let line = line.trim();
    line.len() >= 3 && line.bytes().all(|b| b == b'-')
}

/// The backticks, the language and the tab name of a line that opens a
/// fence, with `None` for a fence without a language or a tab name.
fn fence(line: &str) -> Option<(usize, Option<String>, Option<String>)> {
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }
    let info = rest.trim_start_matches('`');
    let ticks = rest.len() - info.len();
    if ticks < 3 {
        return None;
    }
    let lang = info.split_whitespace().next().map(str::to_string);
    let tab = info
        .split_once(" tab=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map(|(name, _)| name.to_string());
    Some((ticks, lang, tab))
}

/// Whether `line` closes a fence opened with `ticks` backticks: a line of at
/// least that many backticks and nothing else.
fn closes_fence(line: &str, ticks: usize) -> bool {
    let t = line.trim();
    t.len() >= ticks && t.bytes().all(|b| b == b'`')
}

fn delimiter_row(line: &str) -> bool {
    line.contains('|')
        && split_cells(line).iter().all(|c| {
            let c = c.trim();
            c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':')
        })
}

fn split_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').collect()
}

fn cells(line: &str, next: &mut usize) -> Vec<Vec<Inline>> {
    split_cells(line)
        .into_iter()
        .map(|c| inlines(c.trim(), next))
        .collect()
}

/// Leading whitespace in columns, a tab counting two.
fn indent(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum()
}

/// A list item and the column its text starts at.
fn list_item(line: &str, next: &mut usize) -> Option<(Item, usize)> {
    let lead = indent(line);
    let rest = line.trim_start();
    let (number, after) = if let Some(after) = rest
        .strip_prefix(['-', '*', '+'])
        .filter(|a| a.starts_with(' '))
    {
        (None, after)
    } else {
        let digits = rest.chars().take_while(char::is_ascii_digit).count();
        let after = rest[digits..]
            .strip_prefix(['.', ')'])
            .filter(|a| digits > 0 && a.starts_with(' '))?;
        (Some(rest[..digits].parse().ok()?), after)
    };
    let text = after.trim_start();
    let marker = rest.chars().count() - after.chars().count();
    let gap = after.chars().count() - text.chars().count();
    let item = Item {
        depth: lead / 2,
        number,
        text: inlines(text.trim_end(), next),
    };
    Some((item, lead + marker + gap))
}

/// The inlines of one line: a run of backticks opens code that
/// the next run of the same length closes, and `**` opens bold
/// that the next `**` closes. A link is `[text](target)` or
/// `[text](target "title")`, numbered from `*next`. Bold and link
/// text are read again for the inlines inside them. An unmatched
/// run or bracket is text.
fn inlines(line: &str, next: &mut usize) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = line;
    while let Some(start) = rest.find(['`', '*', '[']) {
        text.push_str(&rest[..start]);
        let after = &rest[start..];
        if let Some(inside) = after.strip_prefix('[') {
            match link(after) {
                Some((label, link, tail)) => {
                    if !text.is_empty() {
                        out.push(Inline::Text(std::mem::take(&mut text)));
                    }
                    let index = *next;
                    *next += 1;
                    out.push(Inline::Link {
                        text: inlines(label, next),
                        index,
                        link,
                    });
                    rest = tail;
                }
                None => {
                    text.push('[');
                    rest = inside;
                }
            }
            continue;
        }
        if let Some(body) = after.strip_prefix("**") {
            match body.find("**").filter(|&end| end > 0) {
                Some(end) => {
                    if !text.is_empty() {
                        out.push(Inline::Text(std::mem::take(&mut text)));
                    }
                    out.push(Inline::Bold(inlines(&body[..end], next)));
                    rest = &body[end + 2..];
                }
                None => {
                    text.push_str("**");
                    rest = body;
                }
            }
            continue;
        }
        if let Some(tail) = after.strip_prefix('*') {
            text.push('*');
            rest = tail;
            continue;
        }
        let ticks = after.len() - after.trim_start_matches('`').len();
        let body = &after[ticks..];
        match closing(body, ticks) {
            Some(end) => {
                if !text.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut text)));
                }
                if end > 0 {
                    out.push(Inline::Code(body[..end].to_string()));
                }
                rest = &body[end + ticks..];
            }
            None => {
                text.push_str(&after[..ticks]);
                rest = body;
            }
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        out.push(Inline::Text(text));
    }
    out
}

/// The text, link and remainder of a link at the start of `s`. The
/// text holds no bracket, so no link nests in another. The target ends at
/// whitespace or `)`, and a title may be in either kind of quote.
fn link(s: &str) -> Option<(&str, Link, &str)> {
    let close = s.find(']')?;
    let label = &s[1..close];
    if label.is_empty() || label.contains('[') {
        return None;
    }
    let args = s[close + 1..].strip_prefix('(')?.trim_start();
    let end = args.find(|c: char| c.is_whitespace() || c == ')')?;
    let target = &args[..end];
    if target.is_empty() {
        return None;
    }
    let mut rest = args[end..].trim_start();
    let mut title = None;
    if let Some(quote) = rest.chars().next().filter(|&c| c == '"' || c == '\'') {
        let body = &rest[1..];
        let stop = body.find(quote)?;
        title = Some(body[..stop].to_string());
        rest = body[stop + 1..].trim_start();
    }
    let tail = rest.strip_prefix(')')?;
    let link = Link {
        target: target.to_string(),
        title,
    };
    Some((label, link, tail))
}

/// Where in `s` a run of exactly `ticks` backticks starts.
fn closing(s: &str, ticks: usize) -> Option<usize> {
    let mut at = 0;
    while let Some(start) = s[at..].find('`') {
        let run = &s[at + start..];
        let len = run.len() - run.trim_start_matches('`').len();
        if len == ticks {
            return Some(at + start);
        }
        at += start + len;
    }
    None
}

/// The blocks as lines exactly `width` characters long, a blank
/// line between blocks.
pub fn layout(blocks: &[Block], width: usize) -> Vec<Row> {
    lines(blocks, width)
        .into_iter()
        .map(|(segs, _)| finish(segs, width))
        .collect()
}

/// `layout`, each row with its mark.
pub fn layout_marked(blocks: &[Block], width: usize) -> Vec<(Row, Mark)> {
    lines(blocks, width)
        .into_iter()
        .map(|(segs, mark)| (finish(segs, width), mark))
        .collect()
}

/// `text` laid out at `width` for a terminal: one string per
/// line, trailing blanks dropped, and the screen's styles as
/// escape codes when `color`.
pub fn text(text: &str, width: usize, color: bool) -> Vec<String> {
    let blocks = parse(text);
    let links = links(&blocks);
    lines(&blocks, width)
        .into_iter()
        .map(|(segs, _)| {
            let mut merged = Vec::new();
            for (s, style) in segs {
                push_seg(&mut merged, s, style);
            }
            let mut out = String::new();
            for (s, style) in trimmed(merged, color) {
                let codes = if color { sgr(style) } else { String::new() };
                // a target holding a control character could end the
                // escape early, so such a link is drawn without one
                let target = style
                    .link
                    .filter(|_| color)
                    .and_then(|i| links.get(i))
                    .map(|l| l.target.as_str())
                    .filter(|t| !t.chars().any(char::is_control));
                if let Some(target) = target {
                    out.push_str(&format!("\x1b]8;;{target}\x1b\\"));
                }
                if codes.is_empty() {
                    out.push_str(&s);
                } else {
                    out.push_str(&format!("\x1b[{codes}m{s}\x1b[0m"));
                }
                if target.is_some() {
                    out.push_str("\x1b]8;;\x1b\\");
                }
            }
            out
        })
        .collect()
}

/// `segs` without trailing blanks, keeping those under a
/// background when `color` paints it.
fn trimmed(mut segs: Vec<Piece>, color: bool) -> Vec<Piece> {
    while let Some((s, style)) = segs.last_mut() {
        if color && *style != Style::new() {
            break;
        }
        let keep = s.trim_end().len();
        if keep > 0 {
            s.truncate(keep);
            break;
        }
        segs.pop();
    }
    segs
}

/// The SGR parameters of `style`, empty for the default.
fn sgr(style: Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    if style.bold {
        codes.push("1".to_string());
    }
    if style.dim {
        codes.push("2".to_string());
    }
    if style.underline {
        codes.push("4".to_string());
    }
    if let Some(c) = style.fg {
        codes.push(ansi(c).to_string());
    }
    codes.join(";")
}

/// The foreground parameter of a named colour.
fn ansi(color: Color) -> u8 {
    match color {
        Color::Black => 30,
        Color::Red => 31,
        Color::Green => 32,
        Color::Yellow => 33,
        Color::Blue => 34,
        Color::Magenta => 35,
        Color::Cyan => 36,
        Color::Gray => 37,
        Color::DarkGray => 90,
        Color::LightRed => 91,
        Color::LightGreen => 92,
        Color::LightYellow => 93,
        Color::LightBlue => 94,
        Color::LightMagenta => 95,
        Color::LightCyan => 96,
        Color::White => 97,
    }
}

/// The blocks as lines of at most `width` characters, a blank
/// line between blocks, each with its mark.
fn lines(blocks: &[Block], width: usize) -> Vec<(Vec<Piece>, Mark)> {
    let theme = &THEME;
    let mut out: Vec<(Vec<Piece>, Mark)> = Vec::new();
    for (n, block) in blocks.iter().enumerate() {
        if n > 0 {
            out.push((Vec::new(), BREAK));
        }
        match block {
            Block::Heading { level, text } => heading_lines(*level, text, width, theme, &mut out),
            Block::Paragraph(text) => out.extend(unindented(wrap(&words(text, theme), width))),
            Block::List(items) => list(items, width, theme, &mut out),
            Block::Code {
                lang, lines, wrap, ..
            } => code(lang.as_deref(), lines, *wrap, width, theme, &mut out),
            Block::Table { header, rows } => table(header, rows, width, theme, &mut out),
            Block::Rule => out.push((vec![("─".repeat(width), theme.rule)], BREAK)),
            Block::Quote(inner) => {
                for (row, mark) in lines(inner, width.saturating_sub(2)) {
                    let mut segs = vec![("│ ".to_string(), theme.quote)];
                    segs.extend(row);
                    let lead = mark.lead + 2;
                    out.push((segs, Mark { lead, ..mark }));
                }
            }
        }
    }
    out
}

/// Wrapped rows with no columns before their text.
fn unindented(rows: Vec<(Vec<Piece>, Join)>) -> impl Iterator<Item = (Vec<Piece>, Mark)> {
    rows.into_iter()
        .map(|(segs, join)| (segs, Mark { lead: 0, join }))
}

/// A line cut or padded to exactly `width`, adjacent segments of
/// one style merged into a span.
fn finish(segs: Vec<Piece>, width: usize) -> Row {
    let mut spans: Row = Vec::new();
    let mut room = width;
    for (s, style) in segs {
        let s: String = s.chars().take(room).collect();
        room -= s.chars().count();
        push_seg(&mut spans, s, style);
    }
    push_seg(&mut spans, " ".repeat(room), Style::new());
    spans
}

fn push_seg(to: &mut Vec<Piece>, s: String, style: Style) {
    if s.is_empty() {
        return;
    }
    match to.last_mut() {
        Some((t, last)) if *last == style => t.push_str(&s),
        _ => to.push((s, style)),
    }
}

fn len(segs: &[Piece]) -> usize {
    segs.iter().map(|(s, _)| s.chars().count()).sum()
}

/// The words of a run of inlines: whitespace in text, bold or
/// link text splits them, and inline code joins whatever touches it.
fn words(text: &[Inline], theme: &Theme) -> Vec<Vec<Piece>> {
    let mut out = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    push_words(text, Style::new(), theme, &mut out, &mut word);
    if !word.is_empty() {
        out.push(word);
    }
    out
}

/// `words` for `text` drawn over `style`, adding to the words in `out`
/// and the unfinished `word`.
fn push_words(
    text: &[Inline],
    style: Style,
    theme: &Theme,
    out: &mut Vec<Vec<Piece>>,
    word: &mut Vec<Piece>,
) {
    for inline in text {
        match inline {
            Inline::Code(c) => push_seg(word, c.clone(), style.patch(theme.code)),
            Inline::Bold(inner) => push_words(inner, style.bold(), theme, out, word),
            Inline::Link { text, index, .. } => {
                let link = Style {
                    link: Some(*index),
                    ..theme.link
                };
                push_words(text, style.patch(link), theme, out, word);
            }
            Inline::Text(t) => {
                for ch in t.chars() {
                    if ch.is_whitespace() {
                        if !word.is_empty() {
                            out.push(std::mem::take(word));
                        }
                    } else {
                        push_seg(word, ch.to_string(), style);
                    }
                }
            }
        }
    }
}

/// Greedy word wrap at `width`, a single space between words; a
/// word longer than the width is cut across lines. No words make
/// one empty line. Each line comes with how it follows the one above.
fn wrap(words: &[Vec<Piece>], width: usize) -> Vec<(Vec<Piece>, Join)> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line: Vec<Piece> = Vec::new();
    let mut join = Join::Break;
    for word in words {
        let n = len(word);
        if !line.is_empty() && len(&line) + 1 + n <= width {
            push_seg(&mut line, " ".into(), Style::new());
        } else if !line.is_empty() {
            lines.push((std::mem::take(&mut line), join));
            join = Join::Space;
        }
        if n <= width {
            for (s, style) in word {
                push_seg(&mut line, s.clone(), *style);
            }
            continue;
        }
        for (s, style) in word {
            for ch in s.chars() {
                if len(&line) == width {
                    lines.push((std::mem::take(&mut line), join));
                    join = Join::Word;
                }
                push_seg(&mut line, ch.to_string(), *style);
            }
        }
    }
    lines.push((line, join));
    lines
}

fn heading_lines(
    level: u8,
    text: &[Inline],
    width: usize,
    theme: &Theme,
    out: &mut Vec<(Vec<Piece>, Mark)>,
) {
    let style = match level {
        1 => theme.heading.underline(),
        _ => theme.heading,
    };
    // patched after the wrap, so the spaces between the words
    // carry the underline too
    for (line, join) in wrap(&words(text, theme), width) {
        let line = line
            .into_iter()
            .map(|(s, st)| (s, st.patch(style)))
            .collect();
        out.push((line, Mark { lead: 0, join }));
    }
}

fn list(items: &[Item], width: usize, theme: &Theme, out: &mut Vec<(Vec<Piece>, Mark)>) {
    for (k, item) in items.iter().enumerate() {
        let marker = match item.number {
            Some(n) => {
                let w = siblings(items, k)
                    .filter_map(|s| s.number)
                    .map(|n| n.to_string().len())
                    .max()
                    .unwrap_or(1);
                format!("{n:>w$}. ")
            }
            None if item.depth == 0 => "• ".to_string(),
            None => "▪ ".to_string(),
        };
        let mark = marker.chars().count();
        // the indent gives way before the text does
        let lead = (item.depth * 2).min(width.saturating_sub(mark + 8));
        let col = (lead + mark).min(width.saturating_sub(1));
        for (j, (mut line, join)) in wrap(&words(&item.text, theme), width - col)
            .into_iter()
            .enumerate()
        {
            let mut segs = Vec::new();
            let mark = if j == 0 {
                push_seg(&mut segs, " ".repeat(lead), Style::new());
                push_seg(&mut segs, marker.clone(), theme.bullet);
                Mark { lead: 0, join }
            } else {
                push_seg(&mut segs, " ".repeat(col), Style::new());
                Mark { lead: col, join }
            };
            segs.append(&mut line);
            out.push((segs, mark));
        }
    }
}

/// The items at `items[k]`'s depth in the run around it that no
/// shallower item breaks.
fn siblings(items: &[Item], k: usize) -> impl Iterator<Item = &Item> {
    let depth = items[k].depth;
    let start = items[..k]
        .iter()
        .rposition(|i| i.depth < depth)
        .map_or(0, |p| p + 1);
    let end = items[k..]
        .iter()
        .position(|i| i.depth < depth)
        .map_or(items.len(), |p| k + p);
    items[start..end].iter().filter(move |i| i.depth == depth)
}

fn code(
    lang: Option<&str>,
    lines: &[String],
    wrap: bool,
    width: usize,
    theme: &Theme,
    out: &mut Vec<(Vec<Piece>, Mark)>,
) {
    let inner = width.saturating_sub(2);
    if let Some(lang) = lang {
        let row = vec![
            ("┌ ".into(), theme.rule),
            (lang.chars().take(inner).collect(), theme.lang),
        ];
        out.push((row, GUTTERED));
    }
    let mut lexer = lang.and_then(Lexer::new);
    for line in lines {
        let segs: Vec<Piece> = match &mut lexer {
            Some(lexer) => lexer
                .line(line)
                .into_iter()
                .map(|(s, class)| {
                    let style = match class {
                        Class::Plain => Style::new(),
                        Class::Comment => theme.comment,
                        Class::Str => theme.string,
                        Class::Number => theme.number,
                        Class::Keyword => theme.keyword,
                    };
                    (s, style)
                })
                .collect(),
            None => vec![(line.clone(), Style::new())],
        };
        if wrap && len(&segs) > inner {
            wrapped_code(segs, inner.max(1), theme, out);
            continue;
        }
        let mut shown = vec![("│ ".to_string(), theme.rule)];
        if len(&segs) > inner {
            let mut room = inner.saturating_sub(1);
            for (s, style) in segs {
                let s: String = s.chars().take(room).collect();
                room -= s.chars().count();
                push_seg(&mut shown, s, style);
            }
            push_seg(&mut shown, "…".into(), theme.rule);
        } else {
            shown.extend(segs);
        }
        out.push((shown, GUTTERED));
    }
}

/// One highlighted line split into rows of `inner` characters,
/// each row after the first under a `┆ ` gutter.
fn wrapped_code(segs: Vec<Piece>, inner: usize, theme: &Theme, out: &mut Vec<(Vec<Piece>, Mark)>) {
    let mut row = vec![("│ ".to_string(), theme.rule)];
    let mut mark = GUTTERED;
    let mut room = inner;
    for (s, style) in segs {
        let mut rest = s.as_str();
        while !rest.is_empty() {
            if room == 0 {
                let next = vec![("┆ ".to_string(), theme.rule)];
                out.push((std::mem::replace(&mut row, next), mark));
                mark.join = Join::Word;
                room = inner;
            }
            let end = rest.char_indices().nth(room).map_or(rest.len(), |(i, _)| i);
            room -= rest[..end].chars().count();
            push_seg(&mut row, rest[..end].to_string(), style);
            rest = &rest[end..];
        }
    }
    out.push((row, mark));
}

fn table(
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    width: usize,
    theme: &Theme,
    out: &mut Vec<(Vec<Piece>, Mark)>,
) {
    let cols = header.len();
    let cell = |row: &[Vec<Inline>], c: usize| words(row.get(c).map_or(&[][..], |v| v), theme);
    let mut widths: Vec<usize> = (0..cols)
        .map(|c| {
            std::iter::once(header)
                .chain(rows.iter().map(Vec::as_slice))
                .map(|row| {
                    let w = cell(row, c);
                    len(&wrap(&w, usize::MAX)[0].0)
                })
                .max()
                .unwrap_or(0)
        })
        .collect();
    let gaps = 3 * cols.saturating_sub(1);
    while widths.iter().sum::<usize>() + gaps > width {
        let widest = (0..cols).max_by_key(|&c| (widths[c], usize::MAX - c));
        match widest {
            Some(c) if widths[c] > MIN_COLUMN => widths[c] -= 1,
            _ => return stacked(header, rows, width, theme, out),
        }
    }
    let sep = || (" │ ".to_string(), theme.rule);
    let draw = |row: &[Vec<Inline>], style: Style, out: &mut Vec<(Vec<Piece>, Mark)>| {
        let wrapped: Vec<Vec<Vec<Piece>>> = (0..cols)
            .map(|c| {
                let words: Vec<Vec<Piece>> = cell(row, c)
                    .into_iter()
                    .map(|w| w.into_iter().map(|(s, st)| (s, st.patch(style))).collect())
                    .collect();
                wrap(&words, widths[c])
                    .into_iter()
                    .map(|(line, _)| line)
                    .collect()
            })
            .collect();
        let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
        for l in 0..height {
            let mut segs = Vec::new();
            for c in 0..cols {
                if c > 0 {
                    segs.push(sep());
                }
                let part = wrapped[c].get(l).cloned().unwrap_or_default();
                let fill = widths[c] - len(&part);
                segs.extend(part);
                segs.push((" ".repeat(fill), Style::new()));
            }
            out.push((segs, BREAK));
        }
    };
    draw(header, theme.header, out);
    let rule: Vec<String> = widths.iter().map(|&w| "─".repeat(w)).collect();
    let rule_line = vec![(rule.join("─┼─"), theme.rule)];
    out.push((rule_line, BREAK));
    for row in rows {
        draw(row, Style::new(), out);
    }
}

/// Each row as a record, a `header: cell` line per column.
fn stacked(
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    width: usize,
    theme: &Theme,
    out: &mut Vec<(Vec<Piece>, Mark)>,
) {
    for (n, row) in rows.iter().enumerate() {
        if n > 0 {
            out.push((Vec::new(), BREAK));
        }
        for (c, head) in header.iter().enumerate() {
            let mut label = words(head, theme);
            if let Some(last) = label.last_mut() {
                last.push((":".into(), Style::new()));
            }
            for word in &mut label {
                for (_, style) in word.iter_mut() {
                    *style = style.patch(theme.rule);
                }
            }
            label.extend(words(row.get(c).map_or(&[][..], |v| v), theme));
            out.extend(unindented(wrap(&label, width)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(lines: &[Row]) -> Vec<String> {
        lines
            .iter()
            .map(|row| row.iter().map(|(s, _)| s.as_str()).collect())
            .collect()
    }

    fn padded(lines: &[&str], width: usize) -> Vec<String> {
        lines.iter().map(|l| format!("{l:<width$}")).collect()
    }

    #[test]
    fn every_line_is_exactly_the_width() {
        let text = "# head\n\na `code` b\n\n- item\n  - deeper\n\n```rs\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nanunbrokenwordlongerthanthewidth";
        for width in [3, 10, 30] {
            for line in strings(&render(text, width)) {
                assert_eq!(line.chars().count(), width, "{line:?}");
            }
        }
    }

    #[test]
    fn inline_code_has_the_colour_and_the_space_between_does_not() {
        let lines = render("run `foam prime` now", 30);
        assert_eq!(strings(&lines), padded(&["run foam prime now"], 30));
        let pieces = &lines[0];
        let code = pieces.iter().find(|p| p.0 == "foam prime").unwrap();
        assert_eq!(code.1.fg, Some(Color::Cyan));
        let before = pieces.iter().find(|p| p.0.ends_with("run ")).unwrap();
        assert_eq!(before.1.fg, None);
    }

    #[test]
    fn inline_code_moves_whole_unless_longer_than_the_width() {
        let lines = strings(&render("aaaa `bb cc` d", 8));
        assert_eq!(lines, padded(&["aaaa", "bb cc d"], 8));
        let lines = strings(&render("x `0123456789`", 6));
        assert_eq!(lines, padded(&["x", "012345", "6789"], 6));
    }

    #[test]
    fn a_heading_ends_the_block_before_it_and_is_bold() {
        let blocks = parse("text\n## Two `c`\nmore\n- item\n# One");
        assert_eq!(
            blocks,
            [
                Block::Paragraph(vec![Inline::Text("text".into())]),
                Block::Heading {
                    level: 2,
                    text: vec![Inline::Text("Two ".into()), Inline::Code("c".into())],
                },
                Block::Paragraph(vec![Inline::Text("more".into())]),
                Block::List(vec![Item {
                    depth: 0,
                    number: None,
                    text: vec![Inline::Text("item".into())],
                }]),
                Block::Heading {
                    level: 1,
                    text: vec![Inline::Text("One".into())],
                },
            ]
        );
        let lines = render("# One Two\n\n### Three", 10);
        assert_eq!(strings(&lines), padded(&["One Two", "", "Three"], 10));
        let one = &lines[0][0];
        assert_eq!(one.0, "One Two");
        assert!(one.1.bold && one.1.underline);
        let three = &lines[2][0];
        assert!(three.1.bold);
        assert!(!three.1.underline);
    }

    #[test]
    fn a_line_of_dashes_is_a_rule_and_ends_the_block_before_it() {
        assert_eq!(
            parse("text\n---\n- item\n  ----\n--"),
            [
                Block::Paragraph(vec![Inline::Text("text".into())]),
                Block::Rule,
                Block::List(vec![Item {
                    depth: 0,
                    number: None,
                    text: vec![Inline::Text("item".into())],
                }]),
                Block::Rule,
                Block::Paragraph(vec![Inline::Text("--".into())]),
            ]
        );
        let lines = render("---", 6);
        assert_eq!(strings(&lines), ["──────"]);
        assert!(lines[0][0].1.dim);
    }

    #[test]
    fn four_hashes_or_a_bare_hash_stay_text() {
        for text in ["#### four", "#", "# ", "#nospace"] {
            assert_eq!(
                parse(text),
                [Block::Paragraph(vec![Inline::Text(text.trim().into())])],
                "{text:?}"
            );
        }
    }

    #[test]
    fn terminal_text_drops_trailing_blanks_and_paints_only_in_color() {
        let md = "# Head\n\nsee `x`\n\n```rust\nfn f() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |";
        assert_eq!(
            text(md, 20, false),
            [
                "Head",
                "",
                "see x",
                "",
                "┌ rust",
                "│ fn f() {}",
                "",
                "a │ b",
                "──┼──",
                "1 │ 2",
            ]
        );
        let painted = text(md, 20, true);
        assert_eq!(painted[0], "\x1b[1;4mHead\x1b[0m");
        assert_eq!(painted[2], "see \x1b[36mx\x1b[0m");
        assert_eq!(painted[4], "\x1b[2m┌ rust\x1b[0m");
        assert_eq!(painted[5], "\x1b[2m│ \x1b[0m\x1b[34mfn\x1b[0m f() {}");
        assert_eq!(
            painted[7],
            "\x1b[1ma\x1b[0m\x1b[2m │ \x1b[0m\x1b[1mb\x1b[0m"
        );
    }

    #[test]
    fn a_lone_backtick_is_text() {
        assert_eq!(
            parse("`code` and a ` alone"),
            vec![Block::Paragraph(vec![
                Inline::Code("code".into()),
                Inline::Text(" and a ` alone".into()),
            ])]
        );
        assert_eq!(strings(&render("a ` b", 6)), padded(&["a ` b"], 6));
    }

    #[test]
    fn lines_join_into_one_paragraph_and_blank_lines_separate_blocks() {
        let text = "one\ntwo\n\n\n\nthree\n- a\n\n- b\nfour";
        assert_eq!(
            strings(&render(text, 10)),
            padded(&["one two", "", "three", "", "• a", "• b", "", "four"], 10)
        );
    }

    #[test]
    fn nested_bullets_hang_under_their_text() {
        let text = "- first item wraps here\n  - nested one wraps too\n    and continues\n- last";
        assert_eq!(
            strings(&render(text, 14)),
            padded(
                &[
                    "• first item",
                    "  wraps here",
                    "  ▪ nested one",
                    "    wraps too",
                    "    and",
                    "    continues",
                    "• last",
                ],
                14
            )
        );
        let blocks = parse(text);
        let Block::List(items) = &blocks[0] else {
            panic!("{blocks:?}")
        };
        assert_eq!(items.iter().map(|i| i.depth).collect::<Vec<_>>(), [0, 1, 0]);
    }

    #[test]
    fn a_numbered_list_after_a_bulleted_one_is_its_own_block() {
        let blocks = parse("- a\n- b\n\n1. c\n2. d\n");
        assert_eq!(blocks.len(), 2, "{blocks:#?}");
        assert!(matches!(&blocks[0], Block::List(items) if items.len() == 2));
        assert!(matches!(&blocks[1], Block::List(items) if items[0].number == Some(1)));
    }

    #[test]
    fn a_deep_item_keeps_its_text_at_a_narrow_width() {
        let text = "- a\n  - b\n    - c\n      - d\n        - hello world";
        let lines = strings(&render(text, 16));
        assert!(lines.iter().any(|l| l.contains("▪ hello")), "{lines:#?}");
        assert!(lines.iter().any(|l| l.contains("▪ c")), "{lines:#?}");
    }

    #[test]
    fn numbers_align_to_the_widest_sibling() {
        let text = (1..=10)
            .map(|n| format!("{n}. item {n}"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n  1) sub item that wraps";
        let lines = strings(&render(&text, 16));
        assert_eq!(lines[0], format!("{:<16}", " 1. item 1"));
        assert_eq!(lines[9], format!("{:<16}", "10. item 10"));
        assert_eq!(lines[10], format!("{:<16}", "  1. sub item"));
        assert_eq!(lines[11], format!("{:<16}", "     that wraps"));
        assert_eq!(lines.len(), 12);
    }

    #[test]
    fn a_fence_with_a_language_names_it_over_the_gutter() {
        let lines = render("```rust\nfn main() {}\n\tlet x = 1;\n```\nafter", 16);
        assert_eq!(
            strings(&lines),
            [
                "┌ rust          ",
                "│ fn main() {}  ",
                "│     let x = 1;",
                "                ",
                "after           ",
            ]
        );
        assert_eq!(lines[0][0].0, "┌ rust");
        assert!(lines[0][0].1.dim);
        assert_eq!(lines[1][0].0, "│ ");
        assert!(lines[1][0].1.dim);
    }

    #[test]
    fn a_fence_closes_only_at_a_run_of_backticks_as_long_as_its_opening() {
        let text = "````markdown\nSome text.\n```rust\nfn main() {}\n```\n````\nafter";
        let blocks = parse(text);
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        let Block::Code { lang, lines, .. } = &blocks[0] else {
            panic!("the first block is the fence");
        };
        assert_eq!(lang.as_deref(), Some("markdown"));
        assert_eq!(lines, &["Some text.", "```rust", "fn main() {}", "```"]);
        assert!(matches!(&blocks[1], Block::Paragraph(_)));
        assert_eq!(parse("```\na\n````\nb").len(), 2);
    }

    #[test]
    fn a_fence_names_its_tab_after_the_language() {
        let tab = |text: &str| match &parse(text)[0] {
            Block::Code { lang, tab, .. } => (lang.clone(), tab.clone()),
            _ => panic!("the block is the fence"),
        };
        let owned = (Some("rust".into()), Some("owned".into()));
        assert_eq!(tab("```rust tab=\"owned\"\nx\n```"), owned);
        assert_eq!(tab("```rust other tab=\"owned\" more\nx\n```"), owned);
        assert_eq!(
            tab("```rust tab=\"two words\"\nx\n```").1.as_deref(),
            Some("two words")
        );
        assert_eq!(tab("```rust\nx\n```"), (Some("rust".into()), None));
        assert_eq!(tab("```rust tab=owned\nx\n```").1, None);
        assert_eq!(tab("```rust tab=\"open\nx\n```").1, None);
    }

    #[test]
    fn a_tab_name_is_not_drawn() {
        assert_eq!(
            render("```rust tab=\"owned\"\nlet\n```", 10),
            render("```rust\nlet\n```", 10)
        );
    }

    #[test]
    fn a_known_fence_highlights_and_an_unknown_one_stays_plain() {
        let lines = render("```rust\nfn main() {}\n```", 16);
        let fun = lines[1].iter().find(|p| p.0 == "fn").unwrap();
        assert_eq!(fun.1.fg, Some(Color::Blue));
        let lines = render("```nosuchlang\nfn main() {}\n```", 16);
        assert_eq!(lines[1].len(), 2);
        assert_eq!(lines[1][1].1, Style::new());
    }

    #[test]
    fn a_fence_without_a_language_cuts_long_lines() {
        let lines = render("```\nshort\na line far too long\n```", 10);
        assert_eq!(strings(&lines), ["│ short   ", "│ a line …"]);
        let cut = lines[1].iter().find(|p| p.0 == "…").unwrap();
        assert!(cut.1.dim);
    }

    #[test]
    fn a_wrapped_fence_splits_long_lines_under_a_continuation_gutter() {
        let mut blocks = parse("```\nshort\na line far too long\n```");
        let Block::Code { wrap, .. } = &mut blocks[0] else {
            panic!("the block is the fence");
        };
        *wrap = true;
        let lines = layout(&blocks, 10);
        assert_eq!(
            strings(&lines),
            ["│ short   ", "│ a line f", "┆ ar too l", "┆ ong     "]
        );
        assert!(lines[2][0].1.dim);
    }

    #[test]
    fn a_wrapped_line_keeps_its_colours_across_rows() {
        let mut blocks = parse("```rust\nlet x = \"abcdefgh\";\n```");
        let Block::Code { wrap, .. } = &mut blocks[0] else {
            panic!("the block is the fence");
        };
        *wrap = true;
        let lines = layout(&blocks, 12);
        assert_eq!(
            strings(&lines),
            ["┌ rust      ", "│ let x = \"a", "┆ bcdefgh\"; "]
        );
        assert_eq!(lines[1][1], ("let".to_string(), THEME.keyword));
        let tail = lines[2].iter().find(|p| p.0.starts_with("bcd")).unwrap();
        assert_eq!(tail.1, THEME.string);
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        assert_eq!(
            parse("text\n```sh\nls\n\n- not a list"),
            vec![
                Block::Paragraph(vec![Inline::Text("text".into())]),
                Block::Code {
                    lang: Some("sh".into()),
                    lines: vec!["ls".into(), "".into(), "- not a list".into()],
                    wrap: false,
                    tab: None,
                },
            ]
        );
    }

    #[test]
    fn a_table_header_is_bold_and_its_rows_are_not() {
        let lines = render("| a | b |\n|---|---|\n| 1 | 2 |", 10);
        let bold = |l: &Row, s: &str| l.iter().find(|p| p.0.trim() == s).unwrap().1.bold;
        assert!(bold(&lines[0], "a") && bold(&lines[0], "b"));
        assert!(!bold(&lines[2], "1") && !bold(&lines[2], "2"));
    }

    #[test]
    fn a_table_that_fits_takes_its_natural_widths() {
        let text = "| name | n |\n|:-----|--:|\n| ann | `12` |\nbob | 3";
        let lines = render(text, 20);
        assert_eq!(
            strings(&lines),
            padded(&["name │ n", "─────┼───", "ann  │ 12", "bob  │ 3"], 20)
        );
        assert!(lines[1][0].1.dim);
    }

    #[test]
    fn a_table_too_wide_shrinks_its_widest_column() {
        let text = "| key | value |\n|---|---|\n| a | the long value that has to wrap |";
        assert_eq!(
            strings(&render(text, 20)),
            [
                "key │ value         ",
                "────┼───────────────",
                "a   │ the long value",
                "    │ that has to   ",
                "    │ wrap          ",
            ]
            .map(|l| format!("{l:<20}"))
        );
    }

    #[test]
    fn a_table_too_narrow_for_its_columns_stacks() {
        let text = "| first name | last name |\n|---|---|\n| ann | lee |\n| bob | ray |";
        let lines = render(text, 16);
        assert_eq!(
            strings(&lines),
            padded(
                &[
                    "first name: ann",
                    "last name: lee",
                    "",
                    "first name: bob",
                    "last name: ray",
                ],
                16
            )
        );
        assert!(lines[0][0].1.dim);
    }

    fn bold_pieces(lines: &[Row]) -> Vec<&str> {
        lines
            .iter()
            .flatten()
            .filter(|p| p.1.bold)
            .map(|p| p.0.as_str())
            .collect()
    }

    #[test]
    fn bold_in_a_paragraph_is_bold_and_the_text_around_it_is_not() {
        assert_eq!(
            parse("a **b `c`** d"),
            [Block::Paragraph(vec![
                Inline::Text("a ".into()),
                Inline::Bold(vec![Inline::Text("b ".into()), Inline::Code("c".into())]),
                Inline::Text(" d".into()),
            ])]
        );
        let lines = render("a **bold** d", 20);
        assert_eq!(strings(&lines), padded(&["a bold d"], 20));
        assert_eq!(bold_pieces(&lines), ["bold"]);
    }

    #[test]
    fn bold_in_a_list_item_is_bold() {
        let lines = render("- one **two** three", 30);
        assert_eq!(strings(&lines), padded(&["• one two three"], 30));
        assert_eq!(bold_pieces(&lines), ["• ", "two"]);
    }

    #[test]
    fn bold_in_a_table_cell_is_bold() {
        let lines = render("| a | b |\n|---|---|\n| **x** | y |", 20);
        assert_eq!(bold_pieces(&lines[2..]), ["x"]);
    }

    #[test]
    fn an_unclosed_bold_marker_stays_literal() {
        assert_eq!(
            parse("a **b and * c"),
            [Block::Paragraph(vec![Inline::Text("a **b and * c".into())])]
        );
        assert_eq!(
            parse("****"),
            [Block::Paragraph(vec![Inline::Text("****".into())])]
        );
    }

    #[test]
    fn stars_inside_inline_code_stay_literal() {
        assert_eq!(
            parse("`**a**` and `x`"),
            [Block::Paragraph(vec![
                Inline::Code("**a**".into()),
                Inline::Text(" and ".into()),
                Inline::Code("x".into()),
            ])]
        );
    }

    #[test]
    fn a_quote_holds_blocks_parsed_from_inside_it() {
        assert_eq!(
            parse("> one\n> two\n>\n> - a\n> - b\nafter"),
            [
                Block::Quote(vec![
                    Block::Paragraph(vec![Inline::Text("one two".into())]),
                    Block::List(vec![
                        Item {
                            depth: 0,
                            number: None,
                            text: vec![Inline::Text("a".into())],
                        },
                        Item {
                            depth: 0,
                            number: None,
                            text: vec![Inline::Text("b".into())],
                        },
                    ]),
                ]),
                Block::Paragraph(vec![Inline::Text("after".into())]),
            ]
        );
    }

    #[test]
    fn a_quote_has_a_dim_gutter_and_fits_a_narrow_width() {
        let lines = render("> one two three four five\n>\n> - six seven eight", 20);
        assert_eq!(
            strings(&lines),
            padded(
                &[
                    "│ one two three four",
                    "│ five",
                    "│ ",
                    "│ • six seven eight",
                ],
                20
            )
        );
        for row in &lines {
            assert_eq!(row[0].0, "│ ");
            assert!(row[0].1.dim);
        }
        for width in [1, 2, 3, 20] {
            let md = "> a `code` **b** word\n> ```\n> long code line\n> ```";
            for line in strings(&render(md, width)) {
                assert_eq!(line.chars().count(), width, "{line:?}");
            }
        }
    }

    #[test]
    fn terminal_text_paints_bold_and_the_quote_gutter() {
        assert_eq!(text("a **b**", 10, true), ["a \x1b[1mb\x1b[0m"]);
        assert_eq!(text("a **b**", 10, false), ["a b"]);
        assert_eq!(text("> q", 10, true), ["\x1b[2m│ \x1b[0mq"]);
        assert_eq!(text("> q", 10, false), ["│ q"]);
    }

    fn link_pieces(row: &Row) -> Vec<(&str, Option<usize>)> {
        row.iter()
            .filter(|p| p.1.link.is_some())
            .map(|p| (p.0.as_str(), p.1.link))
            .collect()
    }

    #[test]
    fn a_link_draws_its_text_underlined_and_not_its_target() {
        let lines = render("see [the docs](https://x.test/a) now", 30);
        assert_eq!(strings(&lines), padded(&["see the docs now"], 30));
        let pieces = link_pieces(&lines[0]);
        assert_eq!(pieces, [("the", Some(0)), ("docs", Some(0))]);
        assert!(
            lines[0]
                .iter()
                .filter(|p| p.1.link.is_some())
                .all(|p| p.1.underline)
        );
        let links = links(&parse("see [the docs](https://x.test/a) now"));
        assert_eq!(
            links,
            [Link {
                target: "https://x.test/a".into(),
                title: None
            }]
        );
    }

    #[test]
    fn a_link_title_in_either_quote_is_kept_and_not_drawn() {
        let md = "[a](x \"one two\") [b](y 'three')";
        assert_eq!(strings(&render(md, 10)), padded(&["a b"], 10));
        let titles: Vec<Option<String>> = links(&parse(md)).into_iter().map(|l| l.title).collect();
        assert_eq!(titles, [Some("one two".into()), Some("three".into())]);
    }

    #[test]
    fn links_work_in_a_list_item_a_table_cell_and_a_quote() {
        let md = "- [a](1)\n\n| h | [b](2) |\n|---|---|\n| [c](3) | d |\n\n> [e](4)";
        let targets: Vec<String> = links(&parse(md)).into_iter().map(|l| l.target).collect();
        assert_eq!(targets, ["1", "2", "3", "4"]);
        let shown: String = strings(&render(md, 20)).join("\n");
        for text in ["• a", "b", "c", "│ e"] {
            assert!(shown.contains(text), "{text:?} in {shown:?}");
        }
        assert!(!shown.contains('('));
        let rows = render(md, 20);
        let indices: Vec<usize> = rows.iter().flatten().filter_map(|p| p.1.link).collect();
        assert_eq!(indices, [0, 1, 2, 3]);
    }

    #[test]
    fn unmatched_brackets_and_broken_links_stay_literal() {
        for md in [
            "[a] (x)",
            "[a](x",
            "[a(x)",
            "[](x)",
            "[a]()",
            "[a](x \"t)",
            "[[a]](x) ]",
            "a ] b [",
        ] {
            let lines = strings(&render(md, 30));
            assert_eq!(lines[0].trim_end(), md, "{md}");
            assert!(links(&parse(md)).is_empty(), "{md}");
        }
    }

    #[test]
    fn a_wrapped_link_keeps_its_index_on_every_piece() {
        let lines = render("[one two three four](t) and [five](u)", 9);
        let first: Vec<_> = lines.iter().flat_map(link_pieces).collect();
        assert_eq!(
            first,
            [
                ("one", Some(0)),
                ("two", Some(0)),
                ("three", Some(0)),
                ("four", Some(0)),
                ("five", Some(1)),
            ]
        );
        let long = render("[abcdefghij](t)", 4);
        let cut: Vec<_> = long.iter().flat_map(link_pieces).collect();
        assert_eq!(cut, [("abcd", Some(0)), ("efgh", Some(0)), ("ij", Some(0))]);
    }

    #[test]
    fn the_links_list_follows_document_order() {
        let md = "[a](1) [b](2)\n\n- [c](3)\n\n> [d](4)\n\n[e](5)";
        let targets: Vec<String> = links(&parse(md)).into_iter().map(|l| l.target).collect();
        assert_eq!(targets, ["1", "2", "3", "4", "5"]);
        let found: Vec<usize> = render(md, 30)
            .iter()
            .flatten()
            .filter_map(|p| p.1.link)
            .collect();
        assert_eq!(found, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn terminal_text_underlines_a_link_and_hyperlinks_it_only_in_color() {
        assert_eq!(text("a [b](u) c", 10, false), ["a b c"]);
        assert_eq!(
            text("a [b](u) c", 10, true),
            ["a \x1b]8;;u\x1b\\\x1b[4mb\x1b[0m\x1b]8;;\x1b\\ c"]
        );
    }

    fn piece<'a>(lines: &'a [Row], text: &str) -> &'a Piece {
        lines.iter().flatten().find(|p| p.0 == text).unwrap()
    }

    fn link_to(target: &str) -> Link {
        Link {
            target: target.into(),
            title: None,
        }
    }

    #[test]
    fn bold_in_link_text_is_bold_and_keeps_the_link() {
        assert_eq!(
            parse("[**failed** now](t)"),
            [Block::Paragraph(vec![Inline::Link {
                text: vec![
                    Inline::Bold(vec![Inline::Text("failed".into())]),
                    Inline::Text(" now".into()),
                ],
                index: 0,
                link: link_to("t"),
            }])]
        );
        let lines = render("[**failed** now](t)", 20);
        assert_eq!(strings(&lines), padded(&["failed now"], 20));
        let failed = piece(&lines, "failed").1;
        assert!(failed.bold && failed.underline);
        assert_eq!(failed.link, Some(0));
        let now = piece(&lines, "now").1;
        assert!(!now.bold && now.underline);
        assert_eq!(now.link, Some(0));
    }

    #[test]
    fn inline_code_in_link_text_keeps_its_colour_and_the_link() {
        let lines = render("[`x`](t)", 10);
        assert_eq!(strings(&lines), padded(&["x"], 10));
        let x = piece(&lines, "x").1;
        assert_eq!(x.fg, Some(Color::Cyan));
        assert!(x.underline);
        assert_eq!(x.link, Some(0));
    }

    #[test]
    fn a_link_in_bold_is_bold_and_keeps_its_index() {
        assert_eq!(
            parse("**a [ok](t)**"),
            [Block::Paragraph(vec![Inline::Bold(vec![
                Inline::Text("a ".into()),
                Inline::Link {
                    text: vec![Inline::Text("ok".into())],
                    index: 0,
                    link: link_to("t"),
                },
            ])])]
        );
        let lines = render("**a [ok](t)**", 10);
        assert_eq!(strings(&lines), padded(&["a ok"], 10));
        let ok = piece(&lines, "ok").1;
        assert!(ok.bold && ok.underline);
        assert_eq!(ok.link, Some(0));
        assert!(piece(&lines, "a").1.bold);
        assert_eq!(links(&parse("**a [ok](t)**")), [link_to("t")]);
    }

    #[test]
    fn inline_code_in_bold_is_bold_in_its_colour() {
        let lines = render("**a `b`**", 10);
        let b = piece(&lines, "b").1;
        assert!(b.bold);
        assert_eq!(b.fg, Some(Color::Cyan));
    }

    #[test]
    fn a_link_in_bold_in_a_list_item_or_a_heading_keeps_its_place_in_the_links() {
        let md = "# **[h](1)**\n\n- [a](2) **b [c](3)**\n\n[d](4)";
        let targets: Vec<String> = links(&parse(md)).into_iter().map(|l| l.target).collect();
        assert_eq!(targets, ["1", "2", "3", "4"]);
        let lines = render(md, 20);
        assert_eq!(strings(&lines), padded(&["h", "", "• a b c", "", "d"], 20));
        let found: Vec<(&str, Option<usize>)> = lines.iter().flat_map(link_pieces).collect();
        assert_eq!(
            found,
            [
                ("h", Some(0)),
                ("a", Some(1)),
                ("c", Some(2)),
                ("d", Some(3))
            ]
        );
        assert!(piece(&lines, "h").1.bold);
        assert!(piece(&lines, "c").1.bold);
    }

    #[test]
    fn a_wrapped_link_with_bold_keeps_its_index_on_every_piece() {
        let lines = render("[one **two three** four](t)", 6);
        let found: Vec<(&str, Option<usize>)> = lines.iter().flat_map(link_pieces).collect();
        assert_eq!(
            found,
            [
                ("one", Some(0)),
                ("two", Some(0)),
                ("three", Some(0)),
                ("four", Some(0)),
            ]
        );
    }

    #[test]
    fn markup_in_a_code_block_stays_literal() {
        let lines = strings(&render("```\n[**a**](t) **[b](u)**\n```", 30));
        assert_eq!(lines[0].trim_end(), "│ [**a**](t) **[b](u)**");
        assert!(links(&parse("```\n[a](t)\n```")).is_empty());
    }

    #[test]
    fn terminal_text_paints_bold_link_text_bold_and_underlined() {
        assert_eq!(
            text("[**b**](u)", 10, true),
            ["\x1b]8;;u\x1b\\\x1b[1;4mb\x1b[0m\x1b]8;;\x1b\\"]
        );
    }

    #[test]
    fn a_link_target_with_a_control_character_gets_no_hyperlink() {
        let md = "[b](u\x07v)";
        assert_eq!(text(md, 10, true), ["\x1b[4mb\x1b[0m"]);
    }

    fn marks(text: &str, width: usize) -> Vec<Mark> {
        layout_marked(&parse(text), width)
            .into_iter()
            .map(|(_, mark)| mark)
            .collect()
    }

    fn mark(lead: usize, join: Join) -> Mark {
        Mark { lead, join }
    }

    #[test]
    fn a_row_that_wraps_at_a_space_goes_on_from_the_one_above() {
        assert_eq!(
            marks("one two three", 8),
            [mark(0, Join::Break), mark(0, Join::Space)]
        );
    }

    #[test]
    fn a_word_cut_across_rows_goes_on_inside_the_word() {
        assert_eq!(
            marks("ab cdefghijkl", 5),
            [
                mark(0, Join::Break),
                mark(0, Join::Space),
                mark(0, Join::Word),
            ]
        );
    }

    #[test]
    fn a_blank_line_and_a_new_paragraph_each_start_a_line() {
        assert_eq!(
            marks("one\n\ntwo", 8),
            [
                mark(0, Join::Break),
                mark(0, Join::Break),
                mark(0, Join::Break)
            ]
        );
    }

    #[test]
    fn a_list_items_later_rows_lead_with_its_indent() {
        assert_eq!(
            marks("- one two three", 10),
            [mark(0, Join::Break), mark(2, Join::Space)]
        );
    }

    #[test]
    fn a_quotes_rows_lead_with_its_bar() {
        assert_eq!(
            marks("> one two three", 10),
            [mark(2, Join::Break), mark(2, Join::Space)]
        );
    }

    #[test]
    fn a_wrapped_code_lines_rows_lead_with_the_gutter_and_join_inside_a_word() {
        let mut blocks = parse("```\nabcdefghij\nk\n```");
        if let Block::Code { wrap, .. } = &mut blocks[0] {
            *wrap = true;
        }
        let marks: Vec<Mark> = layout_marked(&blocks, 6)
            .into_iter()
            .map(|(_, mark)| mark)
            .collect();
        assert_eq!(
            marks,
            [
                mark(2, Join::Break),
                mark(2, Join::Word),
                mark(2, Join::Word),
                mark(2, Join::Break),
            ]
        );
    }

    #[test]
    fn marked_rows_are_the_rows_layout_gives() {
        let blocks = parse("# head\n\n- a b c d e f\n\n> q r s t u");
        let rows: Vec<Row> = layout_marked(&blocks, 6)
            .into_iter()
            .map(|(row, _)| row)
            .collect();
        assert_eq!(rows, layout(&blocks, 6));
    }
}
