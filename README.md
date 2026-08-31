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