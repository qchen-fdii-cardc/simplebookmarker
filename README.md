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

To check the bookmark text without writing a PDF, run:

```console
sbm book --dry-run
```

Dry-run mode loads the PDF, parses the bookmark text, reports valid entries,
blank lines, malformed lines, and out-of-range page numbers, then exits without
modifying or writing any PDF.

To export existing PDF bookmarks back to the text format, run:

```console
sbm book --export
```

This writes `book.txt` by default. To choose the exported bookmark file, pass a
path after `--export`. When only `--input` is used, the default export path is
the input PDF path with a `.txt` extension.

```console
sbm book --export existing-bookmarks.txt
```

Existing PDF bookmarks are preserved. When an input entry points to a page that
already has a bookmark, the default behavior is to create another bookmark:

```console
sbm book --on-existing create
```

To update the title of an existing bookmark on the same page instead, use:

```console
sbm book --on-existing update
```

Update mode matches existing same-page bookmarks once each, in document order.
It keeps their hierarchy and changes only their titles. If no unmatched
bookmark exists on that page, a new bookmark is created.

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
