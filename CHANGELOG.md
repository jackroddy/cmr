# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Headings of four `#`, drawn in italics. A `###` heading is now bold and
  italic, so each of the four levels looks different.
- `Document::render` lays out a document as a `Page` under the default
  `Format`. `Document::render_with` lays it out under a given `Format`.
  `Page::rows` returns the rows. `Page::string`, `Page::bytes` and
  `Page`'s `Display` write the rows with SGR codes and OSC 8 hyperlinks,
  one row per line.
- `Format` holds the layout settings. `Format::builder()` returns a
  `FormatBuilder` with a setter for each of `width`, `height`,
  `horizontal`, `vertical`, `code`, `table_rules`, `columns` and `gap`.
  `Format::default()` is 80 columns at the text's natural height.
- `Horizontal` is where the text sits across the width. `Vertical` is
  where it sits in the rows a `height` leaves spare. `Overflow` is whether
  a code line wider than the width wraps, is cut, or widens its block.
- `Block::Table` has a `rules` field, which the parser sets to false. When a
  caller sets it, the table has the header rule again between body rows.
  The new field breaks code that builds `Block::Table` or matches it without
  `..`.
- A laid-out piece's `Style::source` holds the byte of the parsed text its
  text starts at, for text cmr takes verbatim from the source. Bullets,
  bars, gutters, padding and the spaces cmr adds have `None`.
- Italics between single `*` or `_` markers, parsed as `Inline::Italic` and
  drawn with `Style::italic`, which a row's `Display` writes as SGR 3. An
  `_` inside a word, as in `snake_case`, stays as typed, and so does a
  marker with a space just inside it. The new variant and field break code that matches
  `Inline` without a wildcard or builds `Style` without `..`.
- `FormatBuilder::tables` sets what happens to a table too wide for the
  width at the column floor. `Overflow::Wrap`, the default, draws it as
  records. `Keep` lays it out at its narrowest, wider than the width, and
  `Cut` cuts those rows at the width.
- `Block::width` is the columns a code block takes laid out whole, gutter
  and language header included, and `None` for other blocks.
- `FormatBuilder::code_margin` keeps columns free at the end of a code
  block's header row. Under `Overflow::Keep` the block widens to keep them.
- `Row::links` lists the columns each link covers in a row and its index
  into `Document::links`.
- `Row::sources` gives the source byte of the char at each column of a row,
  and `None` where cmr drew the char.
- `Document::render_block` lays out one block of a document alone, its
  links indexed as in the whole document.
- `FormatBuilder::indent` puts blank columns before every row and counts
  them in the row's `lead`. The text is laid out in the width left.
- `FormatBuilder::max_height` cuts long output as `height` does and never
  pads short output.

### Changed

- `Inline::Text` and `Inline::Code` hold the byte their text starts at.
  `Block::Code` has a `sources` field with the byte each line starts at.
  The space that joins two lines of a paragraph is its own `Inline::Text`,
  with `None`. `Block::Code`'s lines keep their tabs. A tab is drawn as
  four spaces. These changes break code that builds the types or matches
  them without `..`.
- The space between two words of one link carries the link's style and
  index. A link of several words is underlined across its spaces, and a
  terminal gets one hyperlink for it, not one per word.
- `parse` returns a `Document`, which holds the text's `blocks` and
  `links`. A caller can change `blocks` before rendering them.
- `Row` is a struct. Its `pieces` hold what a `Row` held before. Its `lead`
  and `join` are the fields `Mark` had. `Row::text` returns the row's
  plain text. `Row`'s `Display` writes the text with SGR codes and an OSC 8
  hyperlink for each link. Both write the full width of the row, trailing
  blanks included.

### Removed

- The functions `render`, `layout`, `layout_marked` and `text`.
  `parse(..).render()` and `parse(..).render_with(format)` take their
  place.
- The `links` function. Its list is now `Document::links`.
- `Mark`. Its fields are on `Row`.

## [0.2.0] - 2026-11-12

### Added

- Links written as `[text](target)` or `[text](target "title")`, with a
  double or single quoted title. Only the text is drawn, underlined, and its
  `Style` carries a `link` index. `links` returns the `Link` targets and titles
  in document order, so `links[i]` belongs to `Some(i)`. `text` also wraps the
  link in an OSC 8 hyperlink when `color` is set. Autolinks and images stay
  as typed.
- `Block::Code` has a `wrap` field, which the parser sets to false. When an
  app sets it, `layout` splits a line wider than the block across rows after
  highlighting, so each piece keeps its colour. A continuation row has `┆` in
  the gutter where the first row has `│`.
- `Block::Code` has a `tab` field. It holds the name from `tab="<name>"` on
  the opening line, after the language. The name may contain spaces.
  `layout` does not draw it.
- `layout_marked` returns the rows of `layout`, each with a `Mark`. The
  mark's `join` says whether the row starts a line of the text, goes on
  after a break at a space, or goes on inside a cut word. Its `lead` counts
  the columns before the row's text: a quote's bar, a code block's gutter,
  or a list item's indent on its later rows. A wrapped code line's `┆` rows
  join inside a word.

### Changed

- `Inline::Bold` and a link's `text` hold `Vec<Inline>` in place of a
  string. Bold may hold inline code and links. Link text may hold bold and
  inline code, but not another link. Styles add up when laid out. Bold link
  text stays underlined and gains bold, and a link in bold is bold. Inline
  code keeps its colour in either one. Every piece of a link keeps the
  link's index, and `links` counts links inside bold in document order.
- A fenced code block closes only at a line of at least as many backticks as
  opened it, and nothing else. A block opened with four backticks now holds
  a three-backtick fence as text, where before it ended there.

## [0.1.0] - 2026-11-08

### Added

- A markdown renderer with no dependencies. It reads headings, paragraphs,
  lists, fenced code with highlighting, tables, rules and inline code, and lays
  them out as styled rows of an exact width, or as plain or ANSI-coloured
  strings.
- Bold text between `**` markers in paragraphs, list items and table cells.
  Bold does not nest, and an unclosed `**` stays as typed.
- Blockquotes from lines that start with `> `. The inside is read as markdown
  again and drawn behind a dim `│ ` gutter.

[Unreleased]: https://github.com/jackroddy/cmr/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/jackroddy/cmr/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/jackroddy/cmr/releases/tag/v0.1.0
