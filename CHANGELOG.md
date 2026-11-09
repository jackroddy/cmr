# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A markdown renderer with no dependencies. It reads headings, paragraphs,
  lists, fenced code with highlighting, tables, rules and inline code, and lays
  them out as styled rows of an exact width, or as plain or ANSI-coloured
  strings.
- Bold text between `**` markers in paragraphs, list items and table cells.
  Bold does not nest, and an unclosed `**` stays as typed.
- Blockquotes from lines that start with `> `. The inside is read as markdown
  again and drawn behind a dim `│ ` gutter.
