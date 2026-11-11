# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Links written as `[text](target)` or `[text](target "title")`, with a
  double or single quoted title. Only the text is drawn, underlined, and its
  `Style` carries a `link` index. `links` returns the `Link` targets and titles
  in document order, so `links[i]` belongs to `Some(i)`. `text` also wraps the
  link in an OSC 8 hyperlink when `color` is set. The text is plain: bold and
  code inside it stay literal. Autolinks and images stay as typed.
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

[Unreleased]: https://github.com/jackroddy/cmr/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jackroddy/cmr/releases/tag/v0.1.0
