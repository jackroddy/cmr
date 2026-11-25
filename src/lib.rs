//! A small markdown subset, parsed into blocks and laid out as
//! lines of an exact width: headings, paragraphs, lists, fenced
//! code, tables, rules, quotes, bold, italics, inline code and links. Everything else stays
//! literal text.

mod hl;

use std::fmt;
use std::ops::RangeInclusive;
use std::rc::Rc;

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
    pub italic: bool,
    pub dim: bool,
    pub underline: bool,
    /// The index into the document's links, for link text.
    pub link: Option<usize>,
    /// The byte of the parsed text the piece's text starts at, when the
    /// text is a slice of it.
    pub source: Option<usize>,
}

impl Style {
    const fn new() -> Style {
        Style {
            fg: None,
            bold: false,
            italic: false,
            dim: false,
            underline: false,
            link: None,
            source: None,
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

    const fn italic(mut self) -> Style {
        self.italic = true;
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
            italic: self.italic || other.italic,
            dim: self.dim || other.dim,
            underline: self.underline || other.underline,
            link: other.link.or(self.link),
            source: other.source.or(self.source),
        }
    }
}

/// A run of text and the style it is drawn in.
pub type Piece = (String, Style);

/// One line of output, exactly as wide as it was laid out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The row's text and styles, left to right.
    pub pieces: Vec<Piece>,
    /// The columns before the text: a quote's bar, a code block's gutter,
    /// or a list item's indent on the rows after its first.
    pub lead: usize,
    pub join: Join,
    links: Rc<[Link]>,
}

impl Row {
    /// The row's text without styles.
    pub fn text(&self) -> String {
        self.pieces.iter().map(|(s, _)| s.as_str()).collect()
    }
}

impl fmt::Display for Row {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut merged = Vec::new();
        for (s, style) in &self.pieces {
            let style = Style {
                source: None,
                ..*style
            };
            push_seg(&mut merged, s.clone(), style);
        }
        for (s, style) in merged {
            // a target holding a control character could end the
            // escape early, so such a link is drawn without one
            let target = style
                .link
                .and_then(|i| self.links.get(i))
                .map(|l| l.target.as_str())
                .filter(|t| !t.chars().any(char::is_control));
            if let Some(target) = target {
                write!(f, "\x1b]8;;{target}\x1b\\")?;
            }
            let codes = sgr(style);
            if codes.is_empty() {
                f.write_str(&s)?;
            } else {
                write!(f, "\x1b[{codes}m{s}\x1b[0m")?;
            }
            if target.is_some() {
                f.write_str("\x1b]8;;\x1b\\")?;
            }
        }
        Ok(())
    }
}

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
#[derive(Clone, Copy)]
struct Mark {
    lead: usize,
    join: Join,
    /// Whether the row is part of a code block kept at its natural width.
    kept: bool,
}

const BREAK: Mark = Mark {
    lead: 0,
    join: Join::Break,
    kept: false,
};

const GUTTERED: Mark = Mark {
    lead: 2,
    join: Join::Break,
    kept: false,
};

/// A run of text inside a paragraph, list item or table cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    /// Text and the byte of the parsed text it starts at, `None` for
    /// the space that joins two lines.
    Text(String, Option<usize>),
    /// Inline code without its backticks, and the byte it starts at.
    Code(String, Option<usize>),
    /// Text between `**` markers, which may hold inline code and links.
    Bold(Vec<Inline>),
    /// Text between single `*` or `_` markers, which may hold inline code
    /// and links.
    Italic(Vec<Inline>),
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
        /// The byte of the parsed text each line starts at.
        sources: Vec<usize>,
        /// Whether a line wider than the block wraps onto continuation
        /// rows; a format's `code` other than `Overflow::Cut` overrides it.
        wrap: bool,
        /// The name given by `tab="<name>"` after the language.
        tab: Option<String>,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        /// Whether a rule runs between body rows, as it does for every
        /// table when a format sets `table_rules`.
        rules: bool,
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

/// A parsed markdown text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    /// The links in document order, so that `links[i]` is the link whose
    /// `Style::link` is `Some(i)`.
    pub links: Vec<Link>,
}

impl Document {
    /// The document laid out under the default format.
    pub fn render(&self) -> Page {
        self.render_with(Format::default())
    }

    /// The document laid out under `format`.
    pub fn render_with(&self, format: Format) -> Page {
        let width = format.width;
        let mut laid = Vec::new();
        let starts = lines(&self.blocks, width, &format, &THEME, &mut laid);
        let links: Rc<[Link]> = Rc::from(self.links.as_slice());
        let row = |segs: Vec<Piece>, mark: Mark| {
            let width = if mark.kept {
                width.max(len(&segs))
            } else {
                width
            };
            Row {
                pieces: finish(segs, width),
                lead: mark.lead,
                join: mark.join,
                links: links.clone(),
            }
        };
        let mut rows: Vec<Row> = laid
            .into_iter()
            .map(|(segs, mark)| row(segs, mark))
            .collect();
        shift(&mut rows, width, format.horizontal);
        if let Some(height) = format.height {
            let blank = row(Vec::new(), BREAK);
            let ellipsis = row(vec![("…".into(), Style::new().dim())], BREAK);
            rows = fit(rows, height, format.vertical, &starts, blank, ellipsis);
        }
        Page { rows }
    }
}

/// Rows shifted right as one block, so the widest row's text sits where
/// `horizontal` puts it.
fn shift(rows: &mut [Row], width: usize, horizontal: Horizontal) {
    let content = |row: &Row| row.text().trim_end_matches(' ').chars().count();
    let widest = rows.iter().map(content).max().unwrap_or(0);
    let spare = width.saturating_sub(widest);
    let by = match horizontal {
        Horizontal::Left => 0,
        Horizontal::Center => spare / 2,
        Horizontal::Right => spare,
    };
    if by == 0 {
        return;
    }
    for row in rows {
        let width = len(&row.pieces);
        let mut segs = vec![(" ".repeat(by), Style::new())];
        segs.append(&mut row.pieces);
        row.pieces = finish(segs, width);
        row.lead += by;
    }
}

/// `rows` padded with `blank` rows or cut to exactly `height`, a cut
/// ending in `ellipsis`. `starts` are the rows the top-level blocks start at.
fn fit(
    mut rows: Vec<Row>,
    height: usize,
    vertical: Vertical,
    starts: &[usize],
    blank: Row,
    ellipsis: Row,
) -> Vec<Row> {
    if rows.len() > height {
        rows.truncate(height);
        if let Some(last) = rows.last_mut() {
            *last = ellipsis;
        }
        return rows;
    }
    let spare = height - rows.len();
    let mut out = Vec::with_capacity(height);
    if vertical == Vertical::Spread && starts.len() > 1 {
        let gaps = starts.len() - 1;
        let mut next = 1;
        for (i, row) in rows.into_iter().enumerate() {
            if next < starts.len() && starts[next] == i {
                let n = spare / gaps + usize::from(next - 1 < spare % gaps);
                out.extend(std::iter::repeat_n(blank.clone(), n));
                next += 1;
            }
            out.push(row);
        }
        return out;
    }
    let above = match vertical {
        Vertical::Top | Vertical::Spread => 0,
        Vertical::Center => spare / 2,
        Vertical::Bottom => spare,
    };
    out.extend(std::iter::repeat_n(blank.clone(), above));
    out.append(&mut rows);
    out.resize(height, blank);
    out
}

/// How a document is laid out and drawn, built with `Format::builder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    width: usize,
    height: Option<usize>,
    horizontal: Horizontal,
    vertical: Vertical,
    code: Overflow,
    table_rules: bool,
    /// The narrowest a table column shrinks to.
    floor: usize,
    /// The widest a table column grows to.
    cap: usize,
    gap: usize,
}

impl Default for Format {
    fn default() -> Format {
        Format {
            width: 80,
            height: None,
            horizontal: Horizontal::Left,
            vertical: Vertical::Top,
            code: Overflow::Cut,
            table_rules: false,
            floor: MIN_COLUMN,
            cap: usize::MAX,
            gap: 1,
        }
    }
}

impl Format {
    /// A builder that starts from the default format.
    pub fn builder() -> FormatBuilder {
        FormatBuilder::default()
    }
}

/// The settings of a `Format`, one setter each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FormatBuilder {
    format: Format,
}

impl FormatBuilder {
    /// The number of columns in a row; 80 by default.
    pub fn width(mut self, width: usize) -> Self {
        self.format.width = width;
        self
    }

    /// The exact number of rows: shorter output is padded with blank rows,
    /// longer output is cut and ends in a row holding `…`.
    pub fn height(mut self, height: usize) -> Self {
        self.format.height = Some(height);
        self
    }

    /// Where the text sits across the width; `Left` by default.
    pub fn horizontal(mut self, horizontal: Horizontal) -> Self {
        self.format.horizontal = horizontal;
        self
    }

    /// Where the spare rows go when a `height` is set; `Top` by default.
    pub fn vertical(mut self, vertical: Vertical) -> Self {
        self.format.vertical = vertical;
        self
    }

    /// What happens to a code line wider than the width; `Cut` by default.
    /// Rows of a block under `Keep` can be wider than the width.
    pub fn code(mut self, code: Overflow) -> Self {
        self.format.code = code;
        self
    }

    /// Whether every table has a rule between its body rows; false by
    /// default.
    pub fn table_rules(mut self, table_rules: bool) -> Self {
        self.format.table_rules = table_rules;
        self
    }

    /// The narrowest a table column shrinks to and the widest it grows to;
    /// `8..=usize::MAX` by default.
    pub fn columns(mut self, columns: RangeInclusive<usize>) -> Self {
        self.format.floor = *columns.start();
        self.format.cap = *columns.end();
        self
    }

    /// The number of blank rows between blocks; 1 by default.
    pub fn gap(mut self, gap: usize) -> Self {
        self.format.gap = gap;
        self
    }

    pub fn build(self) -> Format {
        self.format
    }
}

/// Where text narrower than the width sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Horizontal {
    Left,
    Center,
    Right,
}

/// Where the rows a `height` leaves spare go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vertical {
    Top,
    Center,
    Bottom,
    /// Shared between the top-level blocks, the earlier gaps taking one
    /// more; `Top` for a single block.
    Spread,
}

/// What happens to a code line wider than the width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// The line goes on in continuation rows.
    Wrap,
    /// The line is cut and ends in `…`.
    Cut,
    /// The block is laid out at the width of its longest line.
    Keep,
}

/// A laid-out document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    rows: Vec<Row>,
}

impl Page {
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The rows with SGR codes and OSC 8 hyperlinks, a newline between rows.
    pub fn string(&self) -> String {
        self.to_string()
    }

    /// The bytes of `string`.
    pub fn bytes(&self) -> Vec<u8> {
        self.string().into_bytes()
    }
}

impl fmt::Display for Page {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (n, row) in self.rows.iter().enumerate() {
            if n > 0 {
                f.write_str("\n")?;
            }
            write!(f, "{row}")?;
        }
        Ok(())
    }
}

/// The document of `markdown`, read a line at a time.
pub fn parse(markdown: &str) -> Document {
    let lines: Vec<&str> = markdown.lines().collect();
    let blocks = parse_lines(markdown, &lines, &mut 0);
    let links = links(&blocks);
    Document { blocks, links }
}

fn links(blocks: &[Block]) -> Vec<Link> {
    let mut out = Vec::new();
    for block in blocks {
        match block {
            Block::Heading { text, .. } | Block::Paragraph(text) => collect_links(text, &mut out),
            Block::List(items) => items.iter().for_each(|i| collect_links(&i.text, &mut out)),
            Block::Table { header, rows, .. } => {
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
            Inline::Bold(inner) | Inline::Italic(inner) => collect_links(inner, out),
            Inline::Link { link, .. } => out.push(link.clone()),
            Inline::Text(..) | Inline::Code(..) => {}
        }
    }
}

/// `parse` for `lines`, each a slice of `src`, numbering links from
/// `*next`.
fn parse_lines(src: &str, lines: &[&str], next: &mut usize) -> Vec<Block> {
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
            blocks.push(Block::Quote(parse_lines(src, &inner, next)));
            continue;
        }
        if let Some((ticks, lang, tab)) = fence(line) {
            flush(&mut blocks, &mut para, &mut list);
            let mut code = Vec::new();
            let mut sources = Vec::new();
            i += 1;
            while i < lines.len() && !closes_fence(lines[i], ticks) {
                code.push(lines[i].to_string());
                sources.push(offset(src, lines[i]));
                i += 1;
            }
            blocks.push(Block::Code {
                lang,
                lines: code,
                sources,
                wrap: false,
                tab,
            });
            i += 1;
            continue;
        }
        if line.contains('|') && lines.get(i + 1).is_some_and(|l| delimiter_row(l)) {
            flush(&mut blocks, &mut para, &mut list);
            let header = cells(src, line, next);
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].contains('|') {
                rows.push(cells(src, lines[i], next));
                i += 1;
            }
            blocks.push(Block::Table {
                header,
                rows,
                rules: false,
            });
            continue;
        }
        if let Some((level, rest)) = heading(line) {
            flush(&mut blocks, &mut para, &mut list);
            blocks.push(Block::Heading {
                level,
                text: inlines(src, rest, next),
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
        if let Some((item, col)) = list_item(src, line, next) {
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
                join(&mut last.text, inlines(src, line.trim(), next));
                i += 1;
                continue;
            }
            flush(&mut blocks, &mut para, &mut list);
        }
        join(&mut para, inlines(src, line.trim(), next));
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
        to.push(Inline::Text(" ".into(), None));
    }
    to.extend(line);
}

/// The byte of `src` that `part`, a slice of it, starts at.
fn offset(src: &str, part: &str) -> usize {
    part.as_ptr() as usize - src.as_ptr() as usize
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
        return Some(&line[1..]);
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

fn cells(src: &str, line: &str, next: &mut usize) -> Vec<Vec<Inline>> {
    split_cells(line)
        .into_iter()
        .map(|c| inlines(src, c.trim(), next))
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
fn list_item(src: &str, line: &str, next: &mut usize) -> Option<(Item, usize)> {
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
        text: inlines(src, text.trim_end(), next),
    };
    Some((item, lead + marker + gap))
}

/// The inlines of `line`, a slice of `src`: a run of backticks opens code that
/// the next run of the same length closes, `**` opens bold
/// that the next `**` closes, and a single `*` or `_` opens italics
/// that the next one closes. A link is `[text](target)` or
/// `[text](target "title")`, numbered from `*next`. Bold and link
/// text are read again for the inlines inside them. An unmatched
/// run or bracket is text.
fn inlines(src: &str, line: &str, next: &mut usize) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut from = 0;
    let mut rest = line;
    while let Some(start) = rest.find(['`', '*', '_', '[']) {
        let after = &rest[start..];
        let text = &line[from..line.len() - after.len()];
        if let Some(inside) = after.strip_prefix('[') {
            match link(after) {
                Some((label, link, tail)) => {
                    push_text(&mut out, src, text);
                    let index = *next;
                    *next += 1;
                    out.push(Inline::Link {
                        text: inlines(src, label, next),
                        index,
                        link,
                    });
                    rest = tail;
                    from = line.len() - rest.len();
                }
                None => rest = inside,
            }
            continue;
        }
        if let Some(body) = after.strip_prefix("**") {
            match body.find("**").filter(|&end| end > 0) {
                Some(end) => {
                    push_text(&mut out, src, text);
                    out.push(Inline::Bold(inlines(src, &body[..end], next)));
                    rest = &body[end + 2..];
                    from = line.len() - rest.len();
                }
                None => rest = body,
            }
            continue;
        }
        if let Some(mark) = after.chars().next().filter(|&c| c == '*' || c == '_') {
            let body = &after[1..];
            match emphasis(text, body, mark) {
                Some(end) => {
                    push_text(&mut out, src, text);
                    out.push(Inline::Italic(inlines(src, &body[..end], next)));
                    rest = &body[end + 1..];
                    from = line.len() - rest.len();
                }
                None => rest = body,
            }
            continue;
        }
        let ticks = after.len() - after.trim_start_matches('`').len();
        let body = &after[ticks..];
        match closing(body, ticks) {
            Some(end) => {
                push_text(&mut out, src, text);
                if end > 0 {
                    let source = Some(offset(src, body));
                    out.push(Inline::Code(body[..end].to_string(), source));
                }
                rest = &body[end + ticks..];
                from = line.len() - rest.len();
            }
            None => rest = body,
        }
    }
    push_text(&mut out, src, &line[from..]);
    out
}

/// Where in `body` the italics that `mark` opens end, `None` when it
/// opens none. `before` is the text ahead of `mark`.
fn emphasis(before: &str, body: &str, mark: char) -> Option<usize> {
    let inside_word = |c: Option<char>| mark == '_' && c.is_some_and(char::is_alphanumeric);
    if inside_word(before.chars().last()) || body.starts_with(char::is_whitespace) {
        return None;
    }
    body.char_indices().skip(1).find_map(|(at, c)| {
        let prev = body[..at].chars().last()?;
        let after = body[at + 1..].chars().next();

        // a `*` beside another is half of a bold marker
        let closes = c == mark
            && !prev.is_whitespace()
            && prev != '*'
            && after != Some('*')
            && !inside_word(after);
        closes.then_some(at)
    })
}

/// Push `text`, a slice of `src`, unless it is empty.
fn push_text(out: &mut Vec<Inline>, src: &str, text: &str) {
    if !text.is_empty() {
        out.push(Inline::Text(text.to_string(), Some(offset(src, text))));
    }
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

/// The SGR parameters of `style`, empty for the default.
fn sgr(style: Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    if style.bold {
        codes.push("1".to_string());
    }
    if style.dim {
        codes.push("2".to_string());
    }
    if style.italic {
        codes.push("3".to_string());
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

/// The blocks as lines of at most `width` characters, each with its
/// mark, and the line each block starts at. Lines of a block kept under
/// `Overflow::Keep` can be wider.
fn lines(
    blocks: &[Block],
    width: usize,
    format: &Format,
    theme: &Theme,
    out: &mut Vec<(Vec<Piece>, Mark)>,
) -> Vec<usize> {
    let mut starts = Vec::with_capacity(blocks.len());
    for (n, block) in blocks.iter().enumerate() {
        if n > 0 {
            out.extend(std::iter::repeat_n((Vec::new(), BREAK), format.gap));
        }
        starts.push(out.len());
        match block {
            Block::Heading { level, text } => heading_lines(*level, text, width, theme, out),
            Block::Paragraph(text) => out.extend(unindented(wrap(&words(text, theme), width))),
            Block::List(items) => list(items, width, theme, out),
            Block::Code {
                lang,
                lines,
                sources,
                wrap,
                ..
            } => {
                let overflow = match format.code {
                    Overflow::Cut if *wrap => Overflow::Wrap,
                    overflow => overflow,
                };
                code(lang.as_deref(), lines, sources, overflow, width, theme, out)
            }
            Block::Table {
                header,
                rows,
                rules,
            } => {
                let rules = *rules || format.table_rules;
                let columns = format.floor..=format.cap;
                table(header, rows, rules, columns, width, theme, out)
            }
            Block::Rule => out.push((vec![("─".repeat(width), theme.rule)], BREAK)),
            Block::Quote(inner) => {
                let mut quoted = Vec::new();
                lines(inner, width.saturating_sub(2), format, theme, &mut quoted);
                for (row, mark) in quoted {
                    let mut segs = vec![("│ ".to_string(), theme.quote)];
                    segs.extend(row);
                    let lead = mark.lead + 2;
                    out.push((segs, Mark { lead, ..mark }));
                }
            }
        }
    }
    starts
}

/// Wrapped rows with no columns before their text.
fn unindented(rows: Vec<(Vec<Piece>, Join)>) -> impl Iterator<Item = (Vec<Piece>, Mark)> {
    rows.into_iter()
        .map(|(segs, join)| (segs, Mark { join, ..BREAK }))
}

/// A line cut or padded to exactly `width`, adjacent segments of
/// one style merged into a span.
fn finish(segs: Vec<Piece>, width: usize) -> Vec<Piece> {
    let mut spans = Vec::new();
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
        Some((t, last)) if continues(t, *last, style) => t.push_str(&s),
        _ => to.push((s, style)),
    }
}

/// Whether a piece in `next` may join the piece `text` in `last`: the
/// same look, and the text going on where `text` ends in the source.
fn continues(text: &str, last: Style, next: Style) -> bool {
    let source = match (last.source, next.source) {
        (None, None) => true,
        (Some(at), Some(next)) => at + text.len() == next,
        _ => false,
    };
    source
        && Style {
            source: None,
            ..last
        } == Style {
            source: None,
            ..next
        }
}

fn len(segs: &[Piece]) -> usize {
    segs.iter().map(|(s, _)| s.chars().count()).sum()
}

/// A word to wrap.
#[derive(Default)]
struct Word {
    pieces: Vec<Piece>,
    /// The byte of a space before the word in the source, which a wrap
    /// may draw between this word and the one before.
    space: Option<usize>,
}

/// The words of a run of inlines: whitespace in text, bold or
/// link text splits them, and inline code joins whatever touches it.
fn words(text: &[Inline], theme: &Theme) -> Vec<Word> {
    let mut out = Vec::new();
    let mut word = Word::default();
    push_words(text, Style::new(), theme, &mut out, &mut word);
    if !word.pieces.is_empty() {
        out.push(word);
    }
    out
}

/// `words` for `text` drawn over `style`, adding to the words in `out`
/// and the unfinished `word`.
fn push_words(text: &[Inline], style: Style, theme: &Theme, out: &mut Vec<Word>, word: &mut Word) {
    for inline in text {
        match inline {
            Inline::Code(c, source) => {
                let style = Style {
                    source: *source,
                    ..style.patch(theme.code)
                };
                push_seg(&mut word.pieces, c.clone(), style);
            }
            Inline::Bold(inner) => push_words(inner, style.bold(), theme, out, word),
            Inline::Italic(inner) => push_words(inner, style.italic(), theme, out, word),
            Inline::Link { text, index, .. } => {
                let link = Style {
                    link: Some(*index),
                    ..theme.link
                };
                push_words(text, style.patch(link), theme, out, word);
            }
            Inline::Text(t, source) => {
                for (i, ch) in t.char_indices() {
                    let source = source.map(|at| at + i);
                    if ch.is_whitespace() {
                        if !word.pieces.is_empty() {
                            out.push(std::mem::take(word));
                            word.space = source.filter(|_| ch == ' ');
                        }
                    } else {
                        push_seg(&mut word.pieces, ch.to_string(), Style { source, ..style });
                    }
                }
            }
        }
    }
}

/// Greedy word wrap at `width`, a single space between words; a
/// word longer than the width is cut across lines. No words make
/// one empty line. Each line comes with how it follows the one above.
fn wrap(words: &[Word], width: usize) -> Vec<(Vec<Piece>, Join)> {
    let width = width.max(1);
    let mut lines = Vec::new();
    let mut line: Vec<Piece> = Vec::new();
    let mut join = Join::Break;
    for word in words {
        let n = len(&word.pieces);
        if !line.is_empty() && len(&line) + 1 + n <= width {
            let before = line.last().map(|(_, style)| *style);
            let after = word.pieces.first().map(|(_, style)| *style);
            let space = match (before, after) {
                (Some(before), Some(after))
                    if before.link.is_some() && before.link == after.link =>
                {
                    Style {
                        source: word.space,
                        ..before
                    }
                }
                _ => Style {
                    source: word.space,
                    ..Style::new()
                },
            };
            push_seg(&mut line, " ".into(), space);
        } else if !line.is_empty() {
            lines.push((std::mem::take(&mut line), join));
            join = Join::Space;
        }
        if n <= width {
            for (s, style) in &word.pieces {
                push_seg(&mut line, s.clone(), *style);
            }
            continue;
        }
        for (s, style) in &word.pieces {
            for (i, ch) in s.char_indices() {
                if len(&line) == width {
                    lines.push((std::mem::take(&mut line), join));
                    join = Join::Word;
                }
                let source = style.source.map(|at| at + i);
                push_seg(&mut line, ch.to_string(), Style { source, ..*style });
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
        out.push((line, Mark { join, ..BREAK }));
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
                Mark { join, ..BREAK }
            } else {
                push_seg(&mut segs, " ".repeat(col), Style::new());
                Mark {
                    lead: col,
                    join,
                    kept: false,
                }
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
    sources: &[usize],
    overflow: Overflow,
    width: usize,
    theme: &Theme,
    out: &mut Vec<(Vec<Piece>, Mark)>,
) {
    let mut lexer = lang.and_then(Lexer::new);
    let mut expanded_lines = Vec::with_capacity(lines.len());
    for (k, line) in lines.iter().enumerate() {
        let classed: Vec<Piece> = match &mut lexer {
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
        expanded_lines.push(expanded(classed, sources.get(k).copied()));
    }
    let kept = overflow == Overflow::Keep;
    let width = if kept {
        let header = lang.map_or(0, |l| l.chars().count());
        let longest = expanded_lines.iter().map(|s| len(s)).max().unwrap_or(0);
        width.max(header.max(longest) + 2)
    } else {
        width
    };
    let inner = width.saturating_sub(2);
    let pad = |mut row: Vec<Piece>| {
        if kept {
            let fill = width.saturating_sub(len(&row));
            push_seg(&mut row, " ".repeat(fill), Style::new());
        }
        (row, Mark { kept, ..GUTTERED })
    };
    if let Some(lang) = lang {
        let row = vec![
            ("┌ ".into(), theme.rule),
            (lang.chars().take(inner).collect(), theme.lang),
        ];
        out.push(pad(row));
    }
    for segs in expanded_lines {
        if overflow == Overflow::Wrap && len(&segs) > inner {
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
        out.push(pad(shown));
    }
}

/// The highlighted pieces of a code line that starts at byte `source`,
/// each with the byte it starts at and a tab drawn as four spaces.
fn expanded(classed: Vec<Piece>, source: Option<usize>) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut at = source;
    for (s, style) in classed {
        for (n, part) in s.split('\t').enumerate() {
            if n > 0 {
                push_seg(&mut out, "    ".into(), style);
                at = at.map(|a| a + 1);
            }
            push_seg(
                &mut out,
                part.to_string(),
                Style {
                    source: at,
                    ..style
                },
            );
            at = at.map(|a| a + part.len());
        }
    }
    out
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
            let source = style.source.map(|at| at + s.len() - rest.len());
            push_seg(&mut row, rest[..end].to_string(), Style { source, ..style });
            rest = &rest[end..];
        }
    }
    out.push((row, mark));
}

fn table(
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    rules: bool,
    columns: RangeInclusive<usize>,
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
                .map_or(0, |w| w.min(*columns.end()))
        })
        .collect();
    let gaps = 3 * cols.saturating_sub(1);
    while widths.iter().sum::<usize>() + gaps > width {
        let widest = (0..cols).max_by_key(|&c| (widths[c], usize::MAX - c));
        match widest {
            Some(c) if widths[c] > *columns.start() => widths[c] -= 1,
            _ => return stacked(header, rows, width, theme, out),
        }
    }
    let sep = || (" │ ".to_string(), theme.rule);
    let draw = |row: &[Vec<Inline>], style: Style, out: &mut Vec<(Vec<Piece>, Mark)>| {
        let wrapped: Vec<Vec<Vec<Piece>>> = (0..cols)
            .map(|c| {
                let mut words = cell(row, c);
                for (_, st) in words.iter_mut().flat_map(|w| &mut w.pieces) {
                    *st = st.patch(style);
                }
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
    out.push((rule_line.clone(), BREAK));
    for (n, row) in rows.iter().enumerate() {
        if rules && n > 0 {
            out.push((rule_line.clone(), BREAK));
        }
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
                last.pieces.push((":".into(), Style::new()));
            }
            for (_, style) in label.iter_mut().flat_map(|w| &mut w.pieces) {
                *style = style.patch(theme.rule);
            }
            label.extend(words(row.get(c).map_or(&[][..], |v| v), theme));
            out.extend(unindented(wrap(&label, width)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_at(doc: &Document, width: usize) -> Vec<Row> {
        let format = Format::builder().width(width).build();
        doc.render_with(format).rows().to_vec()
    }

    fn texts(md: &str, format: FormatBuilder) -> Vec<String> {
        strings(parse(md).render_with(format.build()).rows())
    }

    fn strings(lines: &[Row]) -> Vec<String> {
        lines.iter().map(Row::text).collect()
    }

    fn painted(md: &str, width: usize) -> Vec<String> {
        rows_at(&parse(md), width)
            .iter()
            .map(Row::to_string)
            .collect()
    }

    fn padded(lines: &[&str], width: usize) -> Vec<String> {
        lines.iter().map(|l| format!("{l:<width$}")).collect()
    }

    fn look(style: Style) -> Style {
        Style {
            source: None,
            ..style
        }
    }

    fn looks(rows: Vec<Row>) -> Vec<Vec<(String, Style)>> {
        rows.into_iter()
            .map(|row| {
                row.pieces
                    .into_iter()
                    .map(|(s, st)| (s, look(st)))
                    .collect()
            })
            .collect()
    }

    // the line join's space and its neighbours are merged again, so a
    // test can write a paragraph's text as one run
    fn unsourced(blocks: Vec<Block>) -> Vec<Block> {
        blocks
            .into_iter()
            .map(|block| match block {
                Block::Heading { level, text } => Block::Heading {
                    level,
                    text: unsourced_inlines(text),
                },
                Block::Paragraph(text) => Block::Paragraph(unsourced_inlines(text)),
                Block::List(items) => Block::List(
                    items
                        .into_iter()
                        .map(|i| Item {
                            text: unsourced_inlines(i.text),
                            ..i
                        })
                        .collect(),
                ),
                Block::Code {
                    lang,
                    lines,
                    wrap,
                    tab,
                    ..
                } => Block::Code {
                    lang,
                    lines,
                    sources: Vec::new(),
                    wrap,
                    tab,
                },
                Block::Table {
                    header,
                    rows,
                    rules,
                } => Block::Table {
                    header: header.into_iter().map(unsourced_inlines).collect(),
                    rows: rows
                        .into_iter()
                        .map(|r| r.into_iter().map(unsourced_inlines).collect())
                        .collect(),
                    rules,
                },
                Block::Rule => Block::Rule,
                Block::Quote(inner) => Block::Quote(unsourced(inner)),
            })
            .collect()
    }

    fn unsourced_inlines(text: Vec<Inline>) -> Vec<Inline> {
        let mut out: Vec<Inline> = Vec::new();
        for inline in text {
            match inline {
                Inline::Text(t, _) => match out.last_mut() {
                    Some(Inline::Text(last, _)) => last.push_str(&t),
                    _ => out.push(Inline::Text(t, None)),
                },
                Inline::Code(c, _) => out.push(Inline::Code(c, None)),
                Inline::Bold(inner) => out.push(Inline::Bold(unsourced_inlines(inner))),
                Inline::Italic(inner) => out.push(Inline::Italic(unsourced_inlines(inner))),
                Inline::Link { text, index, link } => out.push(Inline::Link {
                    text: unsourced_inlines(text),
                    index,
                    link,
                }),
            }
        }
        out
    }

    #[test]
    fn every_line_is_exactly_the_width() {
        let text = "# head\n\na `code` b\n\n- item\n  - deeper\n\n```rs\nfn main() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nanunbrokenwordlongerthanthewidth";
        for width in [3, 10, 30] {
            for line in strings(&rows_at(&parse(text), width)) {
                assert_eq!(line.chars().count(), width, "{line:?}");
            }
        }
    }

    #[test]
    fn inline_code_has_the_colour_and_the_space_between_does_not() {
        let lines = rows_at(&parse("run `foam prime` now"), 30);
        assert_eq!(strings(&lines), padded(&["run foam prime now"], 30));
        let pieces = &lines[0].pieces;
        let code = pieces.iter().find(|p| p.0 == "foam prime").unwrap();
        assert_eq!(code.1.fg, Some(Color::Cyan));
        let before = pieces.iter().find(|p| p.0.ends_with("run ")).unwrap();
        assert_eq!(before.1.fg, None);
    }

    #[test]
    fn inline_code_moves_whole_unless_longer_than_the_width() {
        let lines = strings(&rows_at(&parse("aaaa `bb cc` d"), 8));
        assert_eq!(lines, padded(&["aaaa", "bb cc d"], 8));
        let lines = strings(&rows_at(&parse("x `0123456789`"), 6));
        assert_eq!(lines, padded(&["x", "012345", "6789"], 6));
    }

    #[test]
    fn a_heading_ends_the_block_before_it_and_is_bold() {
        let blocks = unsourced(parse("text\n## Two `c`\nmore\n- item\n# One").blocks);
        assert_eq!(
            blocks,
            [
                Block::Paragraph(vec![Inline::Text("text".into(), None)]),
                Block::Heading {
                    level: 2,
                    text: vec![
                        Inline::Text("Two ".into(), None),
                        Inline::Code("c".into(), None)
                    ],
                },
                Block::Paragraph(vec![Inline::Text("more".into(), None)]),
                Block::List(vec![Item {
                    depth: 0,
                    number: None,
                    text: vec![Inline::Text("item".into(), None)],
                }]),
                Block::Heading {
                    level: 1,
                    text: vec![Inline::Text("One".into(), None)],
                },
            ]
        );
        let lines = rows_at(&parse("# One Two\n\n### Three"), 10);
        assert_eq!(strings(&lines), padded(&["One Two", "", "Three"], 10));
        let one = &lines[0].pieces[0];
        assert_eq!(one.0, "One Two");
        assert!(one.1.bold && one.1.underline);
        let three = &lines[2].pieces[0];
        assert!(three.1.bold);
        assert!(!three.1.underline);
    }

    #[test]
    fn a_line_of_dashes_is_a_rule_and_ends_the_block_before_it() {
        assert_eq!(
            unsourced(parse("text\n---\n- item\n  ----\n--").blocks),
            [
                Block::Paragraph(vec![Inline::Text("text".into(), None)]),
                Block::Rule,
                Block::List(vec![Item {
                    depth: 0,
                    number: None,
                    text: vec![Inline::Text("item".into(), None)],
                }]),
                Block::Rule,
                Block::Paragraph(vec![Inline::Text("--".into(), None)]),
            ]
        );
        let lines = rows_at(&parse("---"), 6);
        assert_eq!(strings(&lines), ["──────"]);
        assert!(lines[0].pieces[0].1.dim);
    }

    #[test]
    fn four_hashes_or_a_bare_hash_stay_text() {
        for text in ["#### four", "#", "# ", "#nospace"] {
            assert_eq!(
                unsourced(parse(text).blocks),
                [Block::Paragraph(vec![Inline::Text(
                    text.trim().into(),
                    None
                )])],
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_displayed_row_paints_its_styles_and_its_text_is_plain() {
        let md = "# Head\n\nsee `x`\n\n```rust\nfn f() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |";
        assert_eq!(
            strings(&rows_at(&parse(md), 20)),
            padded(
                &[
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
                ],
                20
            )
        );
        let painted = painted(md, 20);
        assert_eq!(painted[0], "\x1b[1;4mHead\x1b[0m                ");
        assert_eq!(painted[2], "see \x1b[36mx\x1b[0m               ");
        assert_eq!(painted[4], "\x1b[2m┌ rust\x1b[0m              ");
        assert_eq!(
            painted[5],
            "\x1b[2m│ \x1b[0m\x1b[34mfn\x1b[0m f() {}         "
        );
        assert_eq!(
            painted[7],
            "\x1b[1ma\x1b[0m\x1b[2m │ \x1b[0m\x1b[1mb\x1b[0m               "
        );
    }

    #[test]
    fn a_lone_backtick_is_text() {
        assert_eq!(
            unsourced(parse("`code` and a ` alone").blocks),
            vec![Block::Paragraph(vec![
                Inline::Code("code".into(), None),
                Inline::Text(" and a ` alone".into(), None),
            ])]
        );
        assert_eq!(strings(&rows_at(&parse("a ` b"), 6)), padded(&["a ` b"], 6));
    }

    #[test]
    fn lines_join_into_one_paragraph_and_blank_lines_separate_blocks() {
        let text = "one\ntwo\n\n\n\nthree\n- a\n\n- b\nfour";
        assert_eq!(
            strings(&rows_at(&parse(text), 10)),
            padded(&["one two", "", "three", "", "• a", "• b", "", "four"], 10)
        );
    }

    #[test]
    fn nested_bullets_hang_under_their_text() {
        let text = "- first item wraps here\n  - nested one wraps too\n    and continues\n- last";
        assert_eq!(
            strings(&rows_at(&parse(text), 14)),
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
        let blocks = parse(text).blocks;
        let Block::List(items) = &blocks[0] else {
            panic!("{blocks:?}")
        };
        assert_eq!(items.iter().map(|i| i.depth).collect::<Vec<_>>(), [0, 1, 0]);
    }

    #[test]
    fn a_numbered_list_after_a_bulleted_one_is_its_own_block() {
        let blocks = parse("- a\n- b\n\n1. c\n2. d\n").blocks;
        assert_eq!(blocks.len(), 2, "{blocks:#?}");
        assert!(matches!(&blocks[0], Block::List(items) if items.len() == 2));
        assert!(matches!(&blocks[1], Block::List(items) if items[0].number == Some(1)));
    }

    #[test]
    fn a_deep_item_keeps_its_text_at_a_narrow_width() {
        let text = "- a\n  - b\n    - c\n      - d\n        - hello world";
        let lines = strings(&rows_at(&parse(text), 16));
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
        let lines = strings(&rows_at(&parse(&text), 16));
        assert_eq!(lines[0], format!("{:<16}", " 1. item 1"));
        assert_eq!(lines[9], format!("{:<16}", "10. item 10"));
        assert_eq!(lines[10], format!("{:<16}", "  1. sub item"));
        assert_eq!(lines[11], format!("{:<16}", "     that wraps"));
        assert_eq!(lines.len(), 12);
    }

    #[test]
    fn a_fence_with_a_language_names_it_over_the_gutter() {
        let lines = rows_at(
            &parse("```rust\nfn main() {}\n\tlet x = 1;\n```\nafter"),
            16,
        );
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
        assert_eq!(lines[0].pieces[0].0, "┌ rust");
        assert!(lines[0].pieces[0].1.dim);
        assert_eq!(lines[1].pieces[0].0, "│ ");
        assert!(lines[1].pieces[0].1.dim);
    }

    #[test]
    fn a_fence_closes_only_at_a_run_of_backticks_as_long_as_its_opening() {
        let text = "````markdown\nSome text.\n```rust\nfn main() {}\n```\n````\nafter";
        let blocks = parse(text).blocks;
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        let Block::Code { lang, lines, .. } = &blocks[0] else {
            panic!("the first block is the fence");
        };
        assert_eq!(lang.as_deref(), Some("markdown"));
        assert_eq!(lines, &["Some text.", "```rust", "fn main() {}", "```"]);
        assert!(matches!(&blocks[1], Block::Paragraph(_)));
        assert_eq!(parse("```\na\n````\nb").blocks.len(), 2);
    }

    #[test]
    fn a_fence_names_its_tab_after_the_language() {
        let tab = |text: &str| match &parse(text).blocks[0] {
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
            looks(rows_at(&parse("```rust tab=\"owned\"\nlet\n```"), 10)),
            looks(rows_at(&parse("```rust\nlet\n```"), 10))
        );
    }

    #[test]
    fn a_known_fence_highlights_and_an_unknown_one_stays_plain() {
        let lines = rows_at(&parse("```rust\nfn main() {}\n```"), 16);
        let fun = lines[1].pieces.iter().find(|p| p.0 == "fn").unwrap();
        assert_eq!(fun.1.fg, Some(Color::Blue));
        let lines = rows_at(&parse("```nosuchlang\nfn main() {}\n```"), 16);
        assert_eq!(lines[1].pieces[1].0, "fn main() {}");
        assert_eq!(look(lines[1].pieces[1].1), Style::new());
    }

    #[test]
    fn a_fence_without_a_language_cuts_long_lines() {
        let lines = rows_at(&parse("```\nshort\na line far too long\n```"), 10);
        assert_eq!(strings(&lines), ["│ short   ", "│ a line …"]);
        let cut = lines[1].pieces.iter().find(|p| p.0 == "…").unwrap();
        assert!(cut.1.dim);
    }

    #[test]
    fn a_wrapped_fence_splits_long_lines_under_a_continuation_gutter() {
        let mut doc = parse("```\nshort\na line far too long\n```");
        let Block::Code { wrap, .. } = &mut doc.blocks[0] else {
            panic!("the block is the fence");
        };
        *wrap = true;
        let lines = rows_at(&doc, 10);
        assert_eq!(
            strings(&lines),
            ["│ short   ", "│ a line f", "┆ ar too l", "┆ ong     "]
        );
        assert!(lines[2].pieces[0].1.dim);
    }

    #[test]
    fn a_wrapped_line_keeps_its_colours_across_rows() {
        let mut doc = parse("```rust\nlet x = \"abcdefgh\";\n```");
        let Block::Code { wrap, .. } = &mut doc.blocks[0] else {
            panic!("the block is the fence");
        };
        *wrap = true;
        let lines = rows_at(&doc, 12);
        assert_eq!(
            strings(&lines),
            ["┌ rust      ", "│ let x = \"a", "┆ bcdefgh\"; "]
        );
        assert_eq!(lines[1].pieces[1].0, "let");
        assert_eq!(look(lines[1].pieces[1].1), THEME.keyword);
        let tail = lines[2]
            .pieces
            .iter()
            .find(|p| p.0.starts_with("bcd"))
            .unwrap();
        assert_eq!(look(tail.1), THEME.string);
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end() {
        assert_eq!(
            parse("text\n```sh\nls\n\n- not a list").blocks,
            vec![
                Block::Paragraph(vec![Inline::Text("text".into(), Some(0))]),
                Block::Code {
                    lang: Some("sh".into()),
                    lines: vec!["ls".into(), "".into(), "- not a list".into()],
                    sources: vec![11, 14, 15],
                    wrap: false,
                    tab: None,
                },
            ]
        );
    }

    #[test]
    fn a_table_header_is_bold_and_its_rows_are_not() {
        let lines = rows_at(&parse("| a | b |\n|---|---|\n| 1 | 2 |"), 10);
        let bold = |l: &Row, s: &str| l.pieces.iter().find(|p| p.0.trim() == s).unwrap().1.bold;
        assert!(bold(&lines[0], "a") && bold(&lines[0], "b"));
        assert!(!bold(&lines[2], "1") && !bold(&lines[2], "2"));
    }

    #[test]
    fn a_table_that_fits_takes_its_natural_widths() {
        let text = "| name | n |\n|:-----|--:|\n| ann | `12` |\nbob | 3";
        let lines = rows_at(&parse(text), 20);
        assert_eq!(
            strings(&lines),
            padded(&["name │ n", "─────┼───", "ann  │ 12", "bob  │ 3"], 20)
        );
        assert!(lines[1].pieces[0].1.dim);
    }

    #[test]
    fn a_table_too_wide_shrinks_its_widest_column() {
        let text = "| key | value |\n|---|---|\n| a | the long value that has to wrap |";
        assert_eq!(
            strings(&rows_at(&parse(text), 20)),
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
    fn a_table_with_rules_has_one_between_body_rows_and_none_after_the_last() {
        let mut doc = parse("| key | value |\n|---|---|\n| a | one two three four |\n| b | five |");
        let Block::Table { rules, .. } = &mut doc.blocks[0] else {
            panic!("expected a table");
        };
        *rules = true;
        assert_eq!(
            strings(&rows_at(&doc, 16)),
            [
                "key │ value     ",
                "────┼───────────",
                "a   │ one two   ",
                "    │ three four",
                "────┼───────────",
                "b   │ five      ",
            ]
        );
    }

    #[test]
    fn a_table_too_narrow_for_its_columns_stacks() {
        let text = "| first name | last name |\n|---|---|\n| ann | lee |\n| bob | ray |";
        let lines = rows_at(&parse(text), 16);
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
        assert!(lines[0].pieces[0].1.dim);
    }

    fn bold_pieces(lines: &[Row]) -> Vec<&str> {
        lines
            .iter()
            .flat_map(|row| &row.pieces)
            .filter(|p| p.1.bold)
            .map(|p| p.0.as_str())
            .collect()
    }

    #[test]
    fn bold_in_a_paragraph_is_bold_and_the_text_around_it_is_not() {
        assert_eq!(
            unsourced(parse("a **b `c`** d").blocks),
            [Block::Paragraph(vec![
                Inline::Text("a ".into(), None),
                Inline::Bold(vec![
                    Inline::Text("b ".into(), None),
                    Inline::Code("c".into(), None)
                ]),
                Inline::Text(" d".into(), None),
            ])]
        );
        let lines = rows_at(&parse("a **bold** d"), 20);
        assert_eq!(strings(&lines), padded(&["a bold d"], 20));
        assert_eq!(bold_pieces(&lines), ["bold"]);
    }

    #[test]
    fn bold_in_a_list_item_is_bold() {
        let lines = rows_at(&parse("- one **two** three"), 30);
        assert_eq!(strings(&lines), padded(&["• one two three"], 30));
        assert_eq!(bold_pieces(&lines), ["• ", "two"]);
    }

    #[test]
    fn bold_in_a_table_cell_is_bold() {
        let lines = rows_at(&parse("| a | b |\n|---|---|\n| **x** | y |"), 20);
        assert_eq!(bold_pieces(&lines[2..]), ["x"]);
    }

    #[test]
    fn text_between_single_stars_or_underscores_is_italic() {
        assert_eq!(
            unsourced(parse("a *b c* and _d_").blocks),
            [Block::Paragraph(vec![
                Inline::Text("a ".into(), None),
                Inline::Italic(vec![Inline::Text("b c".into(), None)]),
                Inline::Text(" and ".into(), None),
                Inline::Italic(vec![Inline::Text("d".into(), None)]),
            ])]
        );
        let lines = rows_at(&parse("a *b c* d"), 20);
        let italic: String = lines[0]
            .pieces
            .iter()
            .filter(|p| p.1.italic)
            .map(|p| p.0.as_str())
            .collect();
        assert_eq!(italic, "bc");
        assert_eq!(painted("a *b*", 10), ["a \x1b[3mb\x1b[0m       "]);
    }

    #[test]
    fn italics_hold_bold_code_and_links() {
        assert_eq!(
            unsourced(parse("*a **b** `c` [d](t)*").blocks),
            [Block::Paragraph(vec![Inline::Italic(vec![
                Inline::Text("a ".into(), None),
                Inline::Bold(vec![Inline::Text("b".into(), None)]),
                Inline::Text(" ".into(), None),
                Inline::Code("c".into(), None),
                Inline::Text(" ".into(), None),
                Inline::Link {
                    text: vec![Inline::Text("d".into(), None)],
                    index: 0,
                    link: link_to("t"),
                },
            ])])]
        );
    }

    #[test]
    fn a_marker_with_a_space_inside_or_an_underscore_in_a_word_stays_literal() {
        for md in ["2 * 3 * 4", "snake_case_name", "a * b", "_ a_", "*a"] {
            assert_eq!(
                unsourced(parse(md).blocks),
                [Block::Paragraph(vec![Inline::Text(md.into(), None)])],
                "{md:?}"
            );
        }
    }

    #[test]
    fn an_unclosed_bold_marker_stays_literal() {
        assert_eq!(
            unsourced(parse("a **b and * c").blocks),
            [Block::Paragraph(vec![Inline::Text(
                "a **b and * c".into(),
                None
            )])]
        );
        assert_eq!(
            unsourced(parse("****").blocks),
            [Block::Paragraph(vec![Inline::Text("****".into(), None)])]
        );
    }

    #[test]
    fn stars_inside_inline_code_stay_literal() {
        assert_eq!(
            unsourced(parse("`**a**` and `x`").blocks),
            [Block::Paragraph(vec![
                Inline::Code("**a**".into(), None),
                Inline::Text(" and ".into(), None),
                Inline::Code("x".into(), None),
            ])]
        );
    }

    #[test]
    fn a_quote_holds_blocks_parsed_from_inside_it() {
        assert_eq!(
            unsourced(parse("> one\n> two\n>\n> - a\n> - b\nafter").blocks),
            [
                Block::Quote(vec![
                    Block::Paragraph(vec![Inline::Text("one two".into(), None)]),
                    Block::List(vec![
                        Item {
                            depth: 0,
                            number: None,
                            text: vec![Inline::Text("a".into(), None)],
                        },
                        Item {
                            depth: 0,
                            number: None,
                            text: vec![Inline::Text("b".into(), None)],
                        },
                    ]),
                ]),
                Block::Paragraph(vec![Inline::Text("after".into(), None)]),
            ]
        );
    }

    #[test]
    fn a_quote_has_a_dim_gutter_and_fits_a_narrow_width() {
        let lines = rows_at(
            &parse("> one two three four five\n>\n> - six seven eight"),
            20,
        );
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
            assert_eq!(row.pieces[0].0, "│ ");
            assert!(row.pieces[0].1.dim);
        }
        for width in [1, 2, 3, 20] {
            let md = "> a `code` **b** word\n> ```\n> long code line\n> ```";
            for line in strings(&rows_at(&parse(md), width)) {
                assert_eq!(line.chars().count(), width, "{line:?}");
            }
        }
    }

    #[test]
    fn a_displayed_row_paints_bold_and_the_quote_gutter() {
        assert_eq!(painted("a **b**", 10), ["a \x1b[1mb\x1b[0m       "]);
        assert_eq!(strings(&rows_at(&parse("a **b**"), 10)), ["a b       "]);
        assert_eq!(painted("> q", 10), ["\x1b[2m│ \x1b[0mq       "]);
        assert_eq!(strings(&rows_at(&parse("> q"), 10)), ["│ q       "]);
    }

    fn link_pieces(row: &Row) -> Vec<(&str, Option<usize>)> {
        row.pieces
            .iter()
            .filter(|p| p.1.link.is_some())
            .map(|p| (p.0.as_str(), p.1.link))
            .collect()
    }

    #[test]
    fn a_link_draws_its_text_underlined_and_not_its_target() {
        let lines = rows_at(&parse("see [the docs](https://x.test/a) now"), 30);
        assert_eq!(strings(&lines), padded(&["see the docs now"], 30));
        let pieces = link_pieces(&lines[0]);
        assert_eq!(pieces, [("the docs", Some(0))]);
        assert!(
            lines[0]
                .pieces
                .iter()
                .filter(|p| p.1.link.is_some())
                .all(|p| p.1.underline)
        );
        let links = parse("see [the docs](https://x.test/a) now").links;
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
        assert_eq!(strings(&rows_at(&parse(md), 10)), padded(&["a b"], 10));
        let titles: Vec<Option<String>> = parse(md).links.into_iter().map(|l| l.title).collect();
        assert_eq!(titles, [Some("one two".into()), Some("three".into())]);
    }

    #[test]
    fn links_work_in_a_list_item_a_table_cell_and_a_quote() {
        let md = "- [a](1)\n\n| h | [b](2) |\n|---|---|\n| [c](3) | d |\n\n> [e](4)";
        let targets: Vec<String> = parse(md).links.into_iter().map(|l| l.target).collect();
        assert_eq!(targets, ["1", "2", "3", "4"]);
        let shown: String = strings(&rows_at(&parse(md), 20)).join("\n");
        for text in ["• a", "b", "c", "│ e"] {
            assert!(shown.contains(text), "{text:?} in {shown:?}");
        }
        assert!(!shown.contains('('));
        let rows = rows_at(&parse(md), 20);
        let indices: Vec<usize> = rows
            .iter()
            .flat_map(|row| &row.pieces)
            .filter_map(|p| p.1.link)
            .collect();
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
            let lines = strings(&rows_at(&parse(md), 30));
            assert_eq!(lines[0].trim_end(), md, "{md}");
            assert!(parse(md).links.is_empty(), "{md}");
        }
    }

    #[test]
    fn a_wrapped_link_keeps_its_index_on_every_piece() {
        let lines = rows_at(&parse("[one two three four](t) and [five](u)"), 9);
        let first: Vec<_> = lines.iter().flat_map(link_pieces).collect();
        assert_eq!(
            first,
            [
                ("one two", Some(0)),
                ("three", Some(0)),
                ("four", Some(0)),
                ("five", Some(1)),
            ]
        );
        let long = rows_at(&parse("[abcdefghij](t)"), 4);
        let cut: Vec<_> = long.iter().flat_map(link_pieces).collect();
        assert_eq!(cut, [("abcd", Some(0)), ("efgh", Some(0)), ("ij", Some(0))]);
    }

    #[test]
    fn the_links_list_follows_document_order() {
        let md = "[a](1) [b](2)\n\n- [c](3)\n\n> [d](4)\n\n[e](5)";
        let targets: Vec<String> = parse(md).links.into_iter().map(|l| l.target).collect();
        assert_eq!(targets, ["1", "2", "3", "4", "5"]);
        let found: Vec<usize> = rows_at(&parse(md), 30)
            .iter()
            .flat_map(|row| &row.pieces)
            .filter_map(|p| p.1.link)
            .collect();
        assert_eq!(found, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn a_displayed_row_underlines_a_link_and_hyperlinks_it_and_its_text_does_not() {
        assert_eq!(strings(&rows_at(&parse("a [b](u) c"), 10)), ["a b c     "]);
        assert_eq!(
            painted("a [b](u) c", 10),
            ["a \x1b]8;;u\x1b\\\x1b[4mb\x1b[0m\x1b]8;;\x1b\\ c     "]
        );
    }

    #[test]
    fn the_space_between_two_words_of_a_link_is_part_of_the_link() {
        let lines = rows_at(&parse("a [b c](u) d"), 20);
        assert_eq!(piece(&lines, "b c").1.link, Some(0));
        assert!(piece(&lines, "b c").1.underline);
        assert_eq!(
            painted("a [b c](u) d", 20),
            ["a \x1b]8;;u\x1b\\\x1b[4mb c\x1b[0m\x1b]8;;\x1b\\ d             "]
        );
    }

    fn piece<'a>(lines: &'a [Row], text: &str) -> &'a Piece {
        lines
            .iter()
            .flat_map(|row| &row.pieces)
            .find(|p| p.0 == text)
            .unwrap()
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
            unsourced(parse("[**failed** now](t)").blocks),
            [Block::Paragraph(vec![Inline::Link {
                text: vec![
                    Inline::Bold(vec![Inline::Text("failed".into(), None)]),
                    Inline::Text(" now".into(), None),
                ],
                index: 0,
                link: link_to("t"),
            }])]
        );
        let lines = rows_at(&parse("[**failed** now](t)"), 20);
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
        let lines = rows_at(&parse("[`x`](t)"), 10);
        assert_eq!(strings(&lines), padded(&["x"], 10));
        let x = piece(&lines, "x").1;
        assert_eq!(x.fg, Some(Color::Cyan));
        assert!(x.underline);
        assert_eq!(x.link, Some(0));
    }

    #[test]
    fn a_link_in_bold_is_bold_and_keeps_its_index() {
        assert_eq!(
            unsourced(parse("**a [ok](t)**").blocks),
            [Block::Paragraph(vec![Inline::Bold(vec![
                Inline::Text("a ".into(), None),
                Inline::Link {
                    text: vec![Inline::Text("ok".into(), None)],
                    index: 0,
                    link: link_to("t"),
                },
            ])])]
        );
        let lines = rows_at(&parse("**a [ok](t)**"), 10);
        assert_eq!(strings(&lines), padded(&["a ok"], 10));
        let ok = piece(&lines, "ok").1;
        assert!(ok.bold && ok.underline);
        assert_eq!(ok.link, Some(0));
        assert!(piece(&lines, "a").1.bold);
        assert_eq!(parse("**a [ok](t)**").links, [link_to("t")]);
    }

    #[test]
    fn inline_code_in_bold_is_bold_in_its_colour() {
        let lines = rows_at(&parse("**a `b`**"), 10);
        let b = piece(&lines, "b").1;
        assert!(b.bold);
        assert_eq!(b.fg, Some(Color::Cyan));
    }

    #[test]
    fn a_link_in_bold_in_a_list_item_or_a_heading_keeps_its_place_in_the_links() {
        let md = "# **[h](1)**\n\n- [a](2) **b [c](3)**\n\n[d](4)";
        let targets: Vec<String> = parse(md).links.into_iter().map(|l| l.target).collect();
        assert_eq!(targets, ["1", "2", "3", "4"]);
        let lines = rows_at(&parse(md), 20);
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
        let lines = rows_at(&parse("[one **two three** four](t)"), 6);
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
        let lines = strings(&rows_at(&parse("```\n[**a**](t) **[b](u)**\n```"), 30));
        assert_eq!(lines[0].trim_end(), "│ [**a**](t) **[b](u)**");
        assert!(parse("```\n[a](t)\n```").links.is_empty());
    }

    #[test]
    fn a_displayed_row_paints_bold_link_text_bold_and_underlined() {
        assert_eq!(
            painted("[**b**](u)", 10),
            ["\x1b]8;;u\x1b\\\x1b[1;4mb\x1b[0m\x1b]8;;\x1b\\         "]
        );
    }

    #[test]
    fn a_displayed_row_hyperlinks_each_link_to_its_own_target() {
        let rows = rows_at(&parse("[a](https://x.test/a)\n\n[b](https://x.test/b)"), 4);
        assert_eq!(
            rows[0].to_string(),
            "\x1b]8;;https://x.test/a\x1b\\\x1b[4ma\x1b[0m\x1b]8;;\x1b\\   "
        );
        assert_eq!(
            rows[2].to_string(),
            "\x1b]8;;https://x.test/b\x1b\\\x1b[4mb\x1b[0m\x1b]8;;\x1b\\   "
        );
    }

    #[test]
    fn a_rows_text_is_its_plain_text_at_the_full_width() {
        let rows = rows_at(&parse("a **b** `c` [d](u)"), 10);
        assert_eq!(rows[0].text(), "a b c d   ");
    }

    #[test]
    fn blocks_edited_before_laying_out_are_the_blocks_laid_out() {
        let mut doc = parse("one\n\ntwo");
        doc.blocks[1] = Block::Rule;
        assert_eq!(strings(&rows_at(&doc, 4)), ["one ", "    ", "────"]);
    }

    #[test]
    fn a_link_target_with_a_control_character_gets_no_hyperlink() {
        let md = "[b](u\x07v)";
        assert_eq!(painted(md, 10), ["\x1b[4mb\x1b[0m         "]);
    }

    fn marks(text: &str, width: usize) -> Vec<(usize, Join)> {
        rows_at(&parse(text), width)
            .into_iter()
            .map(|row| (row.lead, row.join))
            .collect()
    }

    fn mark(lead: usize, join: Join) -> (usize, Join) {
        (lead, join)
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
        let mut doc = parse("```\nabcdefghij\nk\n```");
        if let Block::Code { wrap, .. } = &mut doc.blocks[0] {
            *wrap = true;
        }
        let marks: Vec<(usize, Join)> = rows_at(&doc, 6)
            .into_iter()
            .map(|row| (row.lead, row.join))
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

    // every kind of block, with a paragraph over several lines, a
    // double space, tabs in prose and code, and nesting in a quote
    const VARIED: &str = "# Title with `code`

A paragraph that  spans
several source lines with **bold words**, `inline code`
and a [link text](https://x.test \"t\") at the end.

- first bullet item that wraps
  - nested bullet with **bold**
    continued on a second line

1. numbered one
2. numbered two

| name | value |
|------|-------|
| ann | `12` and more |

> quoted text with `code`
> - a quoted bullet item
>   continued here

```rust
fn main() {\tlet x = \"a long string literal past the width\"; }
\tindented
```

---

tab\tseparated\twords";

    fn sourced(doc: &Document, width: usize) -> Vec<(String, usize)> {
        rows_at(doc, width)
            .into_iter()
            .flat_map(|row| row.pieces)
            .filter_map(|(s, style)| style.source.map(|at| (s, at)))
            .collect()
    }

    #[test]
    fn every_piece_with_a_source_starts_with_its_text_there() {
        let mut doc = parse(VARIED);
        for wrap_code in [false, true] {
            for block in &mut doc.blocks {
                if let Block::Code { wrap, .. } = block {
                    *wrap = wrap_code;
                }
            }
            for width in [12, 20, 40, 80] {
                for (s, at) in sourced(&doc, width) {
                    assert!(
                        VARIED[at..].starts_with(&s),
                        "{s:?} at {at} in width {width}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_word_of_the_prose_has_a_source() {
        let pieces = sourced(&parse(VARIED), 80);
        for word in [
            "Title",
            "code",
            "paragraph",
            "spans",
            "several",
            "bold",
            "words",
            "inline",
            "link",
            "text",
            "end.",
            "first",
            "nested",
            "continued",
            "numbered",
            "name",
            "value",
            "ann",
            "12",
            "more",
            "quoted",
            "here",
            "main",
            "indented",
            "tab",
            "separated",
        ] {
            assert!(
                pieces.iter().any(|(s, _)| s.contains(word)),
                "{word:?} in {pieces:?}"
            );
        }
    }

    fn source_of(rows: &[Row], text: &str) -> Option<usize> {
        rows.iter()
            .flat_map(|row| &row.pieces)
            .find(|p| p.0 == text)
            .unwrap()
            .1
            .source
    }

    #[test]
    fn a_continuation_row_starts_at_the_source_of_its_first_word() {
        let rows = rows_at(&parse("alpha beta\ngamma delta"), 11);
        assert_eq!(rows[1].pieces[0].0, "gamma delta");
        assert_eq!(rows[1].pieces[0].1.source, Some(11));
        let rows = rows_at(&parse("ab cdefghijkl"), 5);
        assert_eq!(rows[2].pieces[0].0, "hijkl");
        assert_eq!(rows[2].pieces[0].1.source, Some(8));
    }

    #[test]
    fn inline_code_starts_past_its_backtick() {
        let rows = rows_at(&parse("run `x y` now"), 20);
        assert_eq!(source_of(&rows, "x y"), Some(5));
    }

    #[test]
    fn link_text_starts_past_its_bracket() {
        let rows = rows_at(&parse("see [the docs](u)"), 20);
        assert_eq!(source_of(&rows, "the docs"), Some(5));
    }

    #[test]
    fn text_in_a_quote_has_sources_in_the_whole_document() {
        let rows = rows_at(&parse("intro\n\n> one\n> - two `three`"), 20);
        assert_eq!(source_of(&rows, "one"), Some(9));
        assert_eq!(source_of(&rows, "two "), Some(17));
        assert_eq!(source_of(&rows, "three"), Some(22));
    }

    #[test]
    fn a_code_line_starts_at_its_line_and_a_tab_has_no_source() {
        let rows = rows_at(&parse("x\n\n```\nlet a = 1;\n\tb\n```"), 20);
        assert_eq!(source_of(&rows, "let a = 1;"), Some(7));
        assert_eq!(source_of(&rows, "    "), None);
        assert_eq!(source_of(&rows, "b"), Some(19));
    }

    #[test]
    fn a_bullet_has_no_source_and_its_text_does() {
        let rows = rows_at(&parse("- item"), 10);
        assert_eq!(source_of(&rows, "• "), None);
        assert_eq!(source_of(&rows, "item"), Some(2));
    }

    #[test]
    fn the_default_format_is_the_builder_with_nothing_set() {
        assert_eq!(Format::default(), Format::builder().build());
    }

    #[test]
    fn render_lays_out_under_the_default_format_at_80_columns() {
        let doc = parse("a");
        assert_eq!(doc.render(), doc.render_with(Format::default()));
        assert_eq!(strings(doc.render().rows()), padded(&["a"], 80));
    }

    #[test]
    fn a_height_pads_below_the_text_under_top() {
        let format = Format::builder().width(3).height(3);
        assert_eq!(texts("a", format), ["a  ", "   ", "   "]);
    }

    #[test]
    fn a_height_pads_above_the_text_under_bottom() {
        let format = Format::builder().width(3).height(3);
        let format = format.vertical(Vertical::Bottom);
        assert_eq!(texts("a", format), ["   ", "   ", "a  "]);
    }

    #[test]
    fn a_height_pads_both_sides_under_center_with_the_odd_row_below() {
        let format = Format::builder().width(3).height(4);
        let format = format.vertical(Vertical::Center);
        assert_eq!(texts("a", format), ["   ", "a  ", "   ", "   "]);
    }

    #[test]
    fn spread_shares_the_spare_rows_between_blocks_the_earlier_gaps_first() {
        let format = Format::builder().width(1).height(8);
        let format = format.vertical(Vertical::Spread);
        let rows = texts("a\n\nb\n\nc", format);
        assert_eq!(rows, ["a", " ", " ", " ", "b", " ", " ", "c"]);
    }

    #[test]
    fn spread_with_one_block_pads_below_it() {
        let format = Format::builder().width(1).height(3);
        let format = format.vertical(Vertical::Spread);
        assert_eq!(texts("a", format), ["a", " ", " "]);
    }

    #[test]
    fn a_height_cuts_long_output_and_ends_it_in_a_dim_ellipsis_row() {
        let format = Format::builder().width(3).height(2).build();
        let page = parse("a\n\nb\n\nc").render_with(format);
        assert_eq!(strings(page.rows()), ["a  ", "…  "]);
        let dim = Style::new().dim();
        assert_eq!(
            page.rows()[1].pieces,
            [("…".into(), dim), ("  ".into(), Style::new())]
        );
    }

    #[test]
    fn center_shifts_every_row_and_its_lead_by_half_the_spare_columns() {
        let format = Format::builder().width(10).horizontal(Horizontal::Center);
        let page = parse("> ab\n\nabc").render_with(format.build());
        assert_eq!(
            strings(page.rows()),
            ["   │ ab   ", "          ", "   abc    "]
        );
        assert_eq!(page.rows()[0].lead, 5);
    }

    #[test]
    fn right_shifts_the_widest_row_against_the_right_edge() {
        let format = Format::builder().width(10).horizontal(Horizontal::Right);
        let page = parse("> ab").render_with(format.build());
        assert_eq!(strings(page.rows()), ["      │ ab"]);
        assert_eq!(page.rows()[0].lead, 8);
    }

    #[test]
    fn keep_lays_a_code_block_out_wider_than_the_width() {
        let md = "```\n0123456789abcdef\nab\n```\n\nafter";
        let format = Format::builder().width(10).code(Overflow::Keep);
        assert_eq!(
            texts(md, format),
            [
                "│ 0123456789abcdef",
                "│ ab              ",
                "          ",
                "after     "
            ]
        );
    }

    #[test]
    fn keep_applies_to_a_code_block_inside_a_quote() {
        let format = Format::builder().width(10).code(Overflow::Keep);
        let rows = texts("> ```\n> 0123456789abcdef\n> ```", format);
        assert_eq!(rows, ["│ │ 0123456789abcdef"]);
    }

    #[test]
    fn wrap_wraps_a_code_block_that_does_not_ask_to() {
        let format = Format::builder().width(10).code(Overflow::Wrap);
        let rows = texts("```\na line far too long\n```", format);
        assert_eq!(rows, ["│ a line f", "┆ ar too l", "┆ ong     "]);
    }

    #[test]
    fn table_rules_draws_a_rule_between_the_body_rows_of_every_table() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
        let format = Format::builder().width(5).table_rules(true);
        assert_eq!(
            texts(md, format),
            ["a │ b", "──┼──", "1 │ 2", "──┼──", "3 │ 4"]
        );
    }

    #[test]
    fn the_column_cap_limits_a_column_that_fits() {
        let md = "| a | b |\n|---|---|\n| one two three four | x |";
        let format = Format::builder().width(40).columns(8..=10);
        assert_eq!(
            texts(md, format),
            padded(
                &[
                    "a          │ b",
                    "───────────┼──",
                    "one two    │ x",
                    "three four │  ",
                ],
                40
            )
        );
    }

    #[test]
    fn a_table_stacks_when_its_columns_would_shrink_below_the_floor() {
        let md = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| x | y |";
        let fits = Format::builder().width(20).columns(8..=usize::MAX);
        assert!(texts(md, fits).iter().any(|row| row.contains('┼')));
        let stacks = Format::builder().width(20).columns(10..=usize::MAX);
        assert_eq!(
            texts(md, stacks),
            ["aaaaaaaaaa: x       ", "bbbbbbbbbb: y       "]
        );
    }

    #[test]
    fn a_gap_of_zero_puts_blocks_on_adjacent_rows_inside_quotes_too() {
        let format = Format::builder().width(3).gap(0);
        assert_eq!(texts("a\n\nb", format), ["a  ", "b  "]);
        assert_eq!(texts("> a\n>\n> b", format), ["│ a", "│ b"]);
    }

    #[test]
    fn a_gap_of_two_puts_two_blank_rows_between_blocks() {
        let format = Format::builder().width(1).gap(2);
        assert_eq!(texts("a\n\nb", format), ["a", " ", " ", "b"]);
    }

    #[test]
    fn a_page_string_is_its_rows_drawn_with_a_newline_between() {
        let page = parse("# a\n\n[b](u)").render_with(Format::builder().width(4).build());
        let joined: Vec<String> = page.rows().iter().map(Row::to_string).collect();
        assert_eq!(page.string(), joined.join("\n"));
        assert_eq!(page.to_string(), page.string());
    }

    #[test]
    fn a_page_bytes_are_the_bytes_of_its_string() {
        let page = parse("**a** `b`").render();
        assert_eq!(page.bytes(), page.string().into_bytes());
    }
}
