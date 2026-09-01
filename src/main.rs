use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};
use lopdf::{Bookmark, Document, Object, ObjectId};
use tempfile::NamedTempFile;

const DEFAULT_EXPORT_PATH: &str = "__sbm_default_export_path__";

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

    /// Replace the input PDF instead of creating a new file
    #[arg(long, visible_alias = "inplace", conflicts_with = "output")]
    in_place: bool,

    /// Parse and report bookmark entries without writing the PDF
    #[arg(long, conflicts_with = "export")]
    dry_run: bool,

    /// Export existing PDF bookmarks to a text file (defaults to NAME.txt)
    #[arg(long, value_name = "TEXT", num_args = 0..=1, default_missing_value = DEFAULT_EXPORT_PATH)]
    export: Option<Option<PathBuf>>,

    /// Discard all existing PDF bookmarks before adding new ones
    #[arg(long, conflicts_with_all = ["export", "on_existing"])]
    from_zero: bool,

    /// How to handle an existing bookmark on the same page
    #[arg(long, value_enum)]
    on_existing: Option<ExistingPolicy>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
enum ExistingPolicy {
    #[default]
    Create,
    Update,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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

struct ExportPaths {
    input: PathBuf,
    output: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
struct ParseReport {
    entries: Vec<Entry>,
    blank_lines: usize,
    malformed_lines: Vec<usize>,
    out_of_range_lines: Vec<(usize, u32)>,
}

impl Cli {
    fn input_path(&self) -> Result<PathBuf, String> {
        let base = self.name.as_deref();
        self.input
            .clone()
            .or_else(|| base.map(|name| PathBuf::from(format!("{name}.pdf"))))
            .ok_or("provide NAME or --input".to_string())
    }

    fn bookmark_path(&self) -> Result<PathBuf, String> {
        let base = self.name.as_deref();
        self.bookmarks
            .clone()
            .or_else(|| base.map(|name| PathBuf::from(format!("{name}.txt"))))
            .ok_or("provide NAME or --bookmarks".to_string())
    }

    fn paths(&self) -> Result<Paths, String> {
        let input = self.input_path()?;
        let bookmarks = self.bookmark_path()?;
        let output = if self.in_place {
            input.clone()
        } else {
            self.output.clone().unwrap_or_else(|| {
                self.name
                    .as_deref()
                    .map(|name| PathBuf::from(format!("{name}_bm.pdf")))
                    .unwrap_or_else(|| {
                        let mut path = input.clone();
                        let stem = input
                            .file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("output");
                        path.set_file_name(format!("{stem}_bm.pdf"));
                        path
                    })
            })
        };

        if !self.in_place && input == output {
            return Err("input and output files must be different".to_string());
        }

        Ok(Paths {
            input,
            bookmarks,
            output,
        })
    }

    fn export_paths(&self) -> Result<ExportPaths, String> {
        let input = self.input_path()?;
        let output = match &self.export {
            Some(Some(path)) if path == Path::new(DEFAULT_EXPORT_PATH) => text_path_for_pdf(&input),
            Some(Some(path)) => path.clone(),
            Some(None) => text_path_for_pdf(&input),
            None => return Err("provide --export to export bookmarks".to_string()),
        };

        Ok(ExportPaths { input, output })
    }

    fn existing_policy(&self) -> ExistingPolicy {
        self.on_existing.unwrap_or(if self.in_place {
            ExistingPolicy::Update
        } else {
            ExistingPolicy::Create
        })
    }
}

fn text_path_for_pdf(input: &Path) -> PathBuf {
    let mut output = input.to_path_buf();
    output.set_extension("txt");
    output
}

fn save_document(
    document: &mut Document,
    input: &Path,
    output: &Path,
) -> Result<(), Box<dyn Error>> {
    if input != output {
        document.save(output)?;
        return Ok(());
    }

    let target = fs::canonicalize(input)?;
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let permissions = fs::metadata(&target)?.permissions();
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.as_file().set_permissions(permissions)?;
    document.save_to(temporary.as_file_mut())?;
    temporary.as_file_mut().sync_all()?;
    temporary.persist(target)?;
    Ok(())
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

fn analyze_entries(contents: &str, max_page: u32) -> ParseReport {
    let mut report = ParseReport {
        entries: Vec::new(),
        blank_lines: 0,
        malformed_lines: Vec::new(),
        out_of_range_lines: Vec::new(),
    };

    for (line_index, line) in contents.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            report.blank_lines += 1;
            continue;
        }

        match parse_line(line) {
            Some(entry) if entry.page <= max_page => report.entries.push(entry),
            Some(entry) => report.out_of_range_lines.push((line_number, entry.page)),
            None => report.malformed_lines.push(line_number),
        }
    }

    report
}

fn format_entries(entries: &[Entry]) -> String {
    let mut contents = String::new();
    for entry in entries {
        contents.push_str(&"    ".repeat(entry.depth));
        contents.push_str(&format!("{}-{}\n", entry.page, entry.title));
    }
    contents
}

fn existing_entries(document: &Document) -> lopdf::Result<Vec<Entry>> {
    match document.get_toc() {
        Ok(toc) => Ok(toc
            .toc
            .into_iter()
            .map(|item| Entry {
                page: item.page as u32,
                title: item.title,
                depth: item.level.saturating_sub(1),
            })
            .collect()),
        Err(lopdf::Error::NoOutline) => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}

fn append_entries(
    document: &mut Document,
    entries: &[Entry],
    pages: &BTreeMap<u32, ObjectId>,
) -> Vec<Option<u32>> {
    let mut parents: Vec<u32> = Vec::new();
    let mut bookmark_ids = Vec::with_capacity(entries.len());

    for entry in entries {
        let Some(&page_id) = pages.get(&entry.page) else {
            bookmark_ids.push(None);
            continue;
        };
        let depth = entry.depth.min(parents.len());
        parents.truncate(depth);
        let parent = depth.checked_sub(1).map(|index| parents[index]);
        let bookmark = Bookmark::new(entry.title.clone(), [0.0, 0.0, 0.0], 0, page_id);
        let bookmark_id = document.add_bookmark(bookmark, parent);
        parents.push(bookmark_id);
        bookmark_ids.push(Some(bookmark_id));
    }

    bookmark_ids
}

fn add_bookmarks(
    document: &mut Document,
    mut existing: Vec<Entry>,
    entries: Vec<Entry>,
    pages: &BTreeMap<u32, ObjectId>,
    policy: ExistingPolicy,
) -> lopdf::Result<()> {
    let mut matches = vec![None; entries.len()];
    if policy == ExistingPolicy::Update {
        let mut used = vec![false; existing.len()];
        for (entry_index, entry) in entries.iter().enumerate() {
            if let Some(existing_index) =
                existing
                    .iter()
                    .enumerate()
                    .position(|(index, existing_entry)| {
                        !used[index] && existing_entry.page == entry.page
                    })
            {
                existing[existing_index].title = entry.title.clone();
                used[existing_index] = true;
                matches[entry_index] = Some(existing_index);
            }
        }
    }

    let existing_ids = append_entries(document, &existing, pages);
    let mut parents: Vec<u32> = Vec::new();

    for (entry_index, entry) in entries.into_iter().enumerate() {
        let Some(&page_id) = pages.get(&entry.page) else {
            continue;
        };
        let depth = entry.depth.min(parents.len());
        parents.truncate(depth);
        let parent = depth.checked_sub(1).map(|index| parents[index]);
        let bookmark_id = matches[entry_index]
            .and_then(|index| existing_ids[index])
            .unwrap_or_else(|| {
                let bookmark = Bookmark::new(entry.title, [0.0, 0.0, 0.0], 0, page_id);
                document.add_bookmark(bookmark, parent)
            });
        parents.push(bookmark_id);
    }

    if let Some(outline_id) = document.build_outline() {
        document
            .catalog_mut()?
            .set("Outlines", Object::Reference(outline_id));
    }

    Ok(())
}

fn print_dry_run_report(
    paths: &Paths,
    max_page: u32,
    existing_count: usize,
    report: &ParseReport,
    policy: ExistingPolicy,
    from_zero: bool,
) {
    println!("Dry run: no PDF will be written");
    println!("Input PDF: {}", paths.input.display());
    println!("Bookmark text: {}", paths.bookmarks.display());
    println!("PDF pages: {max_page}");
    println!("Existing PDF bookmarks: {existing_count}");
    if from_zero {
        println!("Existing bookmark policy: FromZero");
    } else {
        println!("Existing bookmark policy: {policy:?}");
    }
    if paths.input == paths.output {
        println!("Would replace input PDF: {}", paths.input.display());
    } else {
        println!("Would write PDF: {}", paths.output.display());
    }
    println!("Valid entries: {}", report.entries.len());
    for entry in &report.entries {
        println!(
            "  page {} depth {} title {}",
            entry.page, entry.depth, entry.title
        );
    }
    println!("Blank lines: {}", report.blank_lines);
    println!("Malformed lines: {}", report.malformed_lines.len());
    for line_number in &report.malformed_lines {
        println!("  line {line_number}: malformed or unsupported indentation");
    }
    println!("Out-of-range entries: {}", report.out_of_range_lines.len());
    for (line_number, page) in &report.out_of_range_lines {
        println!("  line {line_number}: page {page} is outside 1..={max_page}");
    }
}

fn export_bookmarks(input: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let document = Document::load(input)?;
    let entries = existing_entries(&document)?;
    fs::write(output, format_entries(&entries))?;
    println!(
        "Exported {} bookmark(s) to {}",
        entries.len(),
        output.display()
    );
    Ok(())
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if cli.export.is_some() {
        let paths = cli.export_paths()?;
        return export_bookmarks(&paths.input, &paths.output);
    }

    let policy = cli.existing_policy();
    let dry_run = cli.dry_run;
    let from_zero = cli.from_zero;
    let paths = cli.paths()?;
    let mut document = Document::load(&paths.input)?;
    let pages = document.get_pages();
    let max_page = pages.keys().copied().max().unwrap_or(0);
    let existing = existing_entries(&document)?;
    let contents = fs::read_to_string(&paths.bookmarks)?;
    let report = analyze_entries(&contents, max_page);
    if dry_run {
        print_dry_run_report(&paths, max_page, existing.len(), &report, policy, from_zero);
        return Ok(());
    }

    let entries = report.entries;
    let existing = if from_zero { Vec::new() } else { existing };
    add_bookmarks(&mut document, existing, entries, &pages, policy)?;
    save_document(&mut document, &paths.input, &paths.output)?;
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
            analyze_entries(contents, 100).entries,
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
    fn reports_entry_parse_details_for_dry_run() {
        let contents = "1-Parent\n\n    2-Child\n3-Too far\n  bad indent";

        assert_eq!(
            analyze_entries(contents, 2),
            ParseReport {
                entries: vec![
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
                blank_lines: 1,
                malformed_lines: vec![5],
                out_of_range_lines: vec![(4, 3)],
            }
        );
    }

    #[test]
    fn formats_entries_with_existing_bookmark_syntax() {
        assert_eq!(
            format_entries(&[
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
            ]),
            "1-Parent\n    2-Child\n"
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
    fn resolves_export_paths() {
        let paths = Cli::try_parse_from(["sbm", "book", "--export"])
            .unwrap()
            .export_paths()
            .unwrap();

        assert_eq!(paths.input, PathBuf::from("book.pdf"));
        assert_eq!(paths.output, PathBuf::from("book.txt"));

        let paths = Cli::try_parse_from(["sbm", "--input", "source.pdf", "--export"])
            .unwrap()
            .export_paths()
            .unwrap();

        assert_eq!(paths.input, PathBuf::from("source.pdf"));
        assert_eq!(paths.output, PathBuf::from("source.txt"));

        let paths = Cli::try_parse_from(["sbm", "book", "--export", "out.txt"])
            .unwrap()
            .export_paths()
            .unwrap();

        assert_eq!(paths.input, PathBuf::from("book.pdf"));
        assert_eq!(paths.output, PathBuf::from("out.txt"));
        assert!(Cli::try_parse_from(["sbm", "book", "--dry-run", "--export"]).is_err());
    }

    #[test]
    fn resolves_in_place_output_and_rejects_explicit_output() {
        let paths = Cli::try_parse_from(["sbm", "book", "--in-place"])
            .unwrap()
            .paths()
            .unwrap();

        assert_eq!(paths.input, PathBuf::from("book.pdf"));
        assert_eq!(paths.output, PathBuf::from("book.pdf"));
        assert!(
            Cli::try_parse_from(["sbm", "book", "--inplace"])
                .unwrap()
                .in_place
        );
        assert!(
            Cli::try_parse_from(["sbm", "book", "--in-place", "--output", "other.pdf"]).is_err()
        );
        assert_eq!(
            Cli::try_parse_from(["sbm", "book"])
                .unwrap()
                .existing_policy(),
            ExistingPolicy::Create
        );
        assert_eq!(
            Cli::try_parse_from(["sbm", "book", "--on-existing", "update"])
                .unwrap()
                .existing_policy(),
            ExistingPolicy::Update
        );
        assert_eq!(
            Cli::try_parse_from(["sbm", "book", "--in-place"])
                .unwrap()
                .existing_policy(),
            ExistingPolicy::Update
        );
        assert_eq!(
            Cli::try_parse_from(["sbm", "book", "--in-place", "--on-existing", "create"])
                .unwrap()
                .existing_policy(),
            ExistingPolicy::Create
        );
        assert_eq!(
            Cli::try_parse_from(["sbm", "book", "--in-place", "--on-existing", "update"])
                .unwrap()
                .existing_policy(),
            ExistingPolicy::Update
        );
    }

    #[test]
    fn parses_from_zero_and_rejects_irrelevant_options() {
        assert!(
            Cli::try_parse_from(["sbm", "book", "--from-zero"])
                .unwrap()
                .from_zero
        );
        assert!(Cli::try_parse_from(["sbm", "book", "--from-zero", "--export"]).is_err());
        assert!(
            Cli::try_parse_from(["sbm", "book", "--from-zero", "--on-existing", "update"]).is_err()
        );
    }

    fn two_page_document() -> Document {
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
        document
    }

    fn document_with_existing_outlines() -> Document {
        let mut document = two_page_document();
        let pages = document.get_pages();
        add_bookmarks(
            &mut document,
            Vec::new(),
            vec![
                Entry {
                    page: 1,
                    title: "Old parent".to_string(),
                    depth: 0,
                },
                Entry {
                    page: 2,
                    title: "Old child".to_string(),
                    depth: 1,
                },
            ],
            &pages,
            ExistingPolicy::Create,
        )
        .unwrap();
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        Document::load_mem(&bytes).unwrap()
    }

    #[test]
    fn writes_visible_nested_outlines() {
        let mut document = two_page_document();

        let pages = document.get_pages();
        add_bookmarks(
            &mut document,
            Vec::new(),
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
            ExistingPolicy::Create,
        )
        .unwrap();

        let directory = tempfile::tempdir().unwrap();
        let pdf_path = directory.path().join("book.pdf");
        fs::write(&pdf_path, b"placeholder").unwrap();
        save_document(&mut document, &pdf_path, &pdf_path).unwrap();
        let loaded = Document::load(&pdf_path).unwrap();
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
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn updates_the_title_of_an_existing_same_page_bookmark() {
        let mut document = document_with_existing_outlines();
        let pages = document.get_pages();
        let existing = existing_entries(&document).unwrap();

        add_bookmarks(
            &mut document,
            existing,
            vec![Entry {
                page: 1,
                title: "New parent".to_string(),
                depth: 0,
            }],
            &pages,
            ExistingPolicy::Update,
        )
        .unwrap();

        let toc = document.get_toc().unwrap().toc;
        assert_eq!(toc.len(), 2);
        assert_eq!(toc[0].title, "New parent");
        assert_eq!(toc[0].level, 1);
        assert_eq!(toc[1].title, "Old child");
        assert_eq!(toc[1].level, 2);
    }

    #[test]
    fn creates_a_new_bookmark_when_the_same_page_already_exists() {
        let mut document = document_with_existing_outlines();
        let pages = document.get_pages();
        let existing = existing_entries(&document).unwrap();

        add_bookmarks(
            &mut document,
            existing,
            vec![Entry {
                page: 1,
                title: "New parent".to_string(),
                depth: 0,
            }],
            &pages,
            ExistingPolicy::Create,
        )
        .unwrap();

        let toc = document.get_toc().unwrap().toc;
        assert_eq!(toc.len(), 3);
        assert!(toc.iter().any(|item| item.title == "Old parent"));
        assert!(toc.iter().any(|item| item.title == "New parent"));
        assert!(toc.iter().any(|item| item.title == "Old child"));
    }

    #[test]
    fn rebuilds_bookmarks_from_zero_without_existing_entries() {
        let mut document = document_with_existing_outlines();
        let pages = document.get_pages();

        add_bookmarks(
            &mut document,
            Vec::new(),
            vec![Entry {
                page: 2,
                title: "Only new bookmark".to_string(),
                depth: 0,
            }],
            &pages,
            ExistingPolicy::Create,
        )
        .unwrap();

        let toc = document.get_toc().unwrap().toc;
        assert_eq!(toc.len(), 1);
        assert_eq!(toc[0].title, "Only new bookmark");
        assert_eq!(toc[0].page, 2);
    }
}
