# cmr

A single-crate cargo library: `cmr`, a markdown renderer for the command line.
It parses a small subset of markdown (headings, paragraphs, lists, fenced code,
tables, rules, blockquotes, bold, inline code, links) and lays it out at a given width
as rows of styled text. The name expands to command line markdown renderer.

Design rationale that the code does not state lives in foam's memories, and
pending work lives in foam's issues. `foam prime` prints both at session start.
Until something runs, the README stays at its header and license, and design,
plans and decisions go to foam, never to this file.

## conventions

**Dependencies.** There are none, on purpose. `cmr` hands back plain styled
rows, and each app converts them to its own TUI types. No ratatui or other
version ever appears in the public API.

**Formatting.** Stock rustfmt, with no configuration. There is no
`rustfmt.toml`. Do not add one: the code is written against rustfmt's defaults.

**Comments.** Follow the `rust-comments` skill, and load it before writing or
editing a comment rather than reconstructing the rules from memory. The default
is no comment.

**Tests.** Unit tests stay inline in the file they cover. Test names are full
sentences describing the property, not `test_foo`.

**Branches.** Work is committed on `dev`, in batches. `main` takes a merge from
`dev` only at a release.

**Releases.** Jack says when a release is, never Claude. Until he does, the
version in `Cargo.toml` stays where it is and changelog entries accumulate
under `## [Unreleased]` with `### Added` / `### Changed` / `### Removed`
headings and no date.

## checks

```console
$ cargo fmt --all
$ cargo fmt --all --check
$ cargo clippy --all-targets --all-features
$ cargo test --all-features
```
