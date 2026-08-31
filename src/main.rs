use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use clap::Parser;
use lopdf::{Bookmark, Document, Object, ObjectId};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Base name used to find NAME.pdf and NAME.txt
    name: Option<String>,

    /// Input PDF file (overrides NAME.pdf)
    #[arg(short, long, value_name = "PDF")]
    input: Option<PathBuf>,

    /// Bookmark text file (overrides NAME.txt)
    #[arg(short, long, value_name = "TEXT")]
    bookmarks: Option<PathBuf>,

    /// Output PDF file (defaults to NAME_bm.pdf)
    #[arg(short, long, value_name = "PDF")]
    output: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    page: u32,
    title: String,
    depth: usize,
}

struct Paths {
    input: PathBuf,
    bookmarks: PathBuf,
    output: PathBuf,
}

impl Cli {
    fn paths(self) -> Result<Paths, String> {
        let base = self.name.as_deref();
        let input = self
            .input
            .or_else(|| base.map(|name| PathBuf::from(format!("{name}.pdf"))))
            .ok_or("provide NAME or --input")?;
        let bookmarks = self
            .bookmarks
            .or_else(|| base.map(|name| PathBuf::from(format!("{name}.txt"))))
            .ok_or("provide NAME or --bookmarks")?;
        let output = self.output.unwrap_or_else(|| {
            base.map(|name| PathBuf::from(format!("{name}_bm.pdf")))
                .unwrap_or_else(|| {
                    let mut path = input.clone();
                    let stem = input
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("output");
                    path.set_file_name(format!("{stem}_bm.pdf"));
                    path
                })
        });

        if input == output {
            return Err("input and output files must be different".to_string());
        }

        Ok(Paths {
            input,
            bookmarks,
            output,
        })
    }
}

fn indentation(line: &str) -> Option<(usize, &str)> {
    let prefix_len = line
        .char_indices()
        .find_map(|(index, character)| (!matches!(character, ' ' | '\t')).then_some(index))
        .unwrap_or(line.len());
    let prefix = &line[..prefix_len];
    let mut depth = 0;
    let mut spaces = 0;

    for character in prefix.chars() {
        match character {
            '\t' if spaces == 0 => depth += 1,
            ' ' => {
                spaces += 1;
                if spaces == 4 {
                    depth += 1;
                    spaces = 0;
                }
            }
            _ => return None,
        }
    }

    (spaces == 0).then_some((depth, &line[prefix_len..]))
}

fn parse_line(line: &str) -> Option<Entry> {
    let (depth, content) = indentation(line)?;
    let digit_count = content.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 {
        return None;
    }

    let page = content[..digit_count].parse().ok()?;
    let rest = content[digit_count..].trim_start();
    let title = rest.strip_prefix('-').unwrap_or(rest).trim();
    if page == 0 || title.is_empty() {
        return None;
    }

    Some(Entry {
        page,
        title: title.to_string(),
        depth,
    })
}

fn parse_entries(contents: &str, max_page: u32) -> Vec<Entry> {
    contents
        .lines()
        .filter_map(parse_line)
        .filter(|entry| entry.page <= max_page)
        .collect()
}

fn add_bookmarks(
    document: &mut Document,
    entries: Vec<Entry>,
    pages: &BTreeMap<u32, ObjectId>,
) -> lopdf::Result<()> {
    let mut parents: Vec<u32> = Vec::new();

    for entry in entries {
        let Some(&page_id) = pages.get(&entry.page) else {
            continue;
        };
        let depth = entry.depth.min(parents.len());
        parents.truncate(depth);
        let parent = depth.checked_sub(1).map(|index| parents[index]);
        let bookmark = Bookmark::new(entry.title, [0.0, 0.0, 0.0], 0, page_id);
        let bookmark_id = document.add_bookmark(bookmark, parent);
        parents.push(bookmark_id);
    }

    if let Some(outline_id) = document.build_outline() {
        document
            .catalog_mut()?
            .set("Outlines", Object::Reference(outline_id));
    }

    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let paths = Cli::parse().paths()?;
    let mut document = Document::load(&paths.input)?;
    let pages = document.get_pages();
    let max_page = pages.keys().copied().max().unwrap_or(0);
    let contents = fs::read_to_string(&paths.bookmarks)?;
    let entries = parse_entries(&contents, max_page);
    add_bookmarks(&mut document, entries, &pages)?;
    document.save(&paths.output)?;
    println!("Wrote {}", paths.output.display());
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn parses_supported_formats_and_filters_invalid_entries() {
        let contents = "20 ONE\n    22-Child\n\t23 - Tab child\n  24 bad indent\n0 zero\n101 too far\nnot an entry";

        assert_eq!(
            parse_entries(contents, 100),
            vec![
                Entry {
                    page: 20,
                    title: "ONE".to_string(),
                    depth: 0,
                },
                Entry {
                    page: 22,
                    title: "Child".to_string(),
                    depth: 1,
                },
                Entry {
                    page: 23,
                    title: "Tab child".to_string(),
                    depth: 1,
                },
            ]
        );
    }

    #[test]
    fn resolves_default_paths() {
        let paths = Cli::try_parse_from(["sbm", "book"])
            .unwrap()
            .paths()
            .unwrap();

        assert_eq!(paths.input, PathBuf::from("book.pdf"));
        assert_eq!(paths.bookmarks, PathBuf::from("book.txt"));
        assert_eq!(paths.output, PathBuf::from("book_bm.pdf"));
    }

    #[test]
    fn writes_visible_nested_outlines() {
        let mut document = Document::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_ids: Vec<Object> = (0..2)
            .map(|_| {
                document
                    .add_object(dictionary! {
                        "Type" => "Page",
                        "Parent" => pages_id,
                        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                    })
                    .into()
            })
            .collect();
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids,
                "Count" => 2,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);

        let pages = document.get_pages();
        add_bookmarks(
            &mut document,
            vec![
                Entry {
                    page: 1,
                    title: "Parent".to_string(),
                    depth: 0,
                },
                Entry {
                    page: 2,
                    title: "Child".to_string(),
                    depth: 1,
                },
            ],
            &pages,
        )
        .unwrap();

        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        let loaded = Document::load_mem(&bytes).unwrap();
        let outline_id = loaded
            .catalog()
            .unwrap()
            .get(b"Outlines")
            .unwrap()
            .as_reference()
            .unwrap();
        let outline = loaded.get_object(outline_id).unwrap().as_dict().unwrap();
        assert_eq!(outline.get(b"Count").unwrap().as_i64().unwrap(), 1);
        let first_id = outline.get(b"First").unwrap().as_reference().unwrap();
        let first = loaded.get_object(first_id).unwrap().as_dict().unwrap();
        assert!(first.get(b"First").is_ok());
    }
}
