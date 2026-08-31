# simplebookmarker

`simplebookmarker` provides the `sbm` command, which adds PDF bookmarks from an
indented plain-text table of contents.

## Installation

```console
cargo install simplebookmarker
```

## Usage

Given `book.pdf` and `book.txt`, run:

```console
sbm book
```

The result is written to `book_bm.pdf`.

To replace `book.pdf` atomically instead of creating a new file, run:

```console
sbm book --in-place
```

`--inplace` is also accepted. In-place mode writes and syncs a temporary file
in the same directory before replacing the original PDF. It cannot be combined
with `--output`.

Input, bookmark, and output paths can also be set explicitly:

```console
sbm --input source.pdf --bookmarks contents.txt --output result.pdf
```

Run `sbm --help` for all options.

## Bookmark format

Each valid line starts with a one-based PDF page number and a title. A hyphen
between the page number and title is optional. Indent child bookmarks with one
tab or four spaces per level.

```text
1-Introduction
    3-Background
        5-History
10 First chapter
```

Blank and malformed lines are ignored. Entries with page zero or a page number
beyond the end of the PDF are also ignored. If indentation skips a level, the
entry is attached at the deepest available level.

## License

MIT

## Release process

Run the **Release** workflow from GitHub Actions and choose `patch`, `minor`,
or `major`. It updates `Cargo.toml` and `Cargo.lock`, runs all checks, commits
the new version to `main`, pushes a matching `vX.Y.Z` tag, publishes to
crates.io, and creates the GitHub Release. Publishing uses the
`CARGO_REGISTRY_TOKEN` secret in the `crates.io` environment.

See [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) for the complete development
and release checklist.
