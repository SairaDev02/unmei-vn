//! The format page and the engine's interface tables cannot disagree (ticket
//! #107, UI plugin spec §5).
//!
//! [`docs/ui-plugin-format.md`](../../docs/ui-plugin-format.md) is the owner of
//! each surface's interface, and [`ui::SURFACES`] is the set of rules a plugin's
//! tree is judged by. This test parses the page's per-surface tables and
//! compares them with the tables, so a name added on one side alone fails here
//! rather than in an author's plugin. The copy key rows are compared with
//! [`copy::KEYS`] as well, because the page shows each key's placeholders and its
//! default text.

use std::collections::BTreeMap;
use std::path::Path;

use unmei_engine::copy;
use unmei_engine::ui::{self, ListSpec, Numbering, SurfaceSpec};

/// One row of a surface's table on the page.
struct Row {
    kind: String,
    name: String,
    list: Option<String>,
    placeholders: Vec<String>,
    shows: String,
}

/// The page's surface sections, by surface id.
fn page_sections() -> BTreeMap<String, Vec<Row>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/ui-plugin-format.md");
    let page = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} reads: {error}", path.display()));
    let mut sections: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in page.lines() {
        if let Some(id) = surface_heading(line) {
            sections.insert(id.clone(), Vec::new());
            current = Some(id);
            continue;
        }
        // Any other heading ends the table a surface section holds.
        if line.starts_with('#') {
            current = None;
            continue;
        }
        let Some(section) = current.as_ref() else {
            continue;
        };
        if let Some(row) = table_row(line) {
            sections.get_mut(section).expect("a section").push(row);
        }
    }
    sections
}

/// The surface id a `### `id`` heading names.
fn surface_heading(line: &str) -> Option<String> {
    let id = line.strip_prefix("### `")?.trim_end().strip_suffix('`')?;
    Some(id.to_string())
}

/// One row of a markdown table, or `None` for a line that is not a row, the
/// header, or the separator under it.
fn table_row(line: &str) -> Option<Row> {
    if !line.starts_with('|') {
        return None;
    }
    let cells: Vec<&str> = line
        .trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(str::trim)
        .collect();
    if cells.len() != 5 {
        return None;
    }
    if cells[0] == "Kind" || cells[0].starts_with("---") {
        return None;
    }
    let placeholders = if cells[3] == "—" {
        Vec::new()
    } else {
        cells[3]
            .split(',')
            .map(|name| name.trim().trim_matches('`').to_string())
            .collect()
    };
    Some(Row {
        kind: cells[0].to_string(),
        name: cells[1].trim_matches('`').to_string(),
        list: (cells[2] != "—").then(|| cells[2].trim_matches('`').to_string()),
        placeholders,
        shows: cells[4].trim_matches('`').to_string(),
    })
}

fn names<'a>(rows: &'a [Row], kind: &str) -> Vec<&'a str> {
    let mut names: Vec<&str> = rows
        .iter()
        .filter(|row| row.kind == kind)
        .map(|row| row.name.as_str())
        .collect();
    names.sort_unstable();
    names
}

fn sorted<'a>(names: &[&'a str]) -> Vec<&'a str> {
    let mut names = names.to_vec();
    names.sort_unstable();
    names
}

/// The item names of one kind, for one list.
fn item_names<'a>(rows: &'a [Row], kind: &str, list: &str) -> Vec<&'a str> {
    let mut names: Vec<&str> = rows
        .iter()
        .filter(|row| row.kind == kind && row.list.as_deref() == Some(list))
        .map(|row| row.name.as_str())
        .collect();
    names.sort_unstable();
    names
}

fn check_surface(spec: &SurfaceSpec, rows: &[Row]) {
    // A list also fills a required slot, so its row answers that slot too.
    let mut slots = names(rows, "required slot");
    slots.extend(names(rows, "list"));
    slots.sort_unstable();
    assert_eq!(
        slots,
        sorted(spec.slots),
        "`{}`: the page's required slots and lists",
        spec.id
    );
    assert_eq!(
        names(rows, "optional slot"),
        sorted(spec.optional_slots),
        "`{}`: the page's optional slots",
        spec.id
    );
    assert_eq!(
        names(rows, "required control"),
        sorted(spec.controls),
        "`{}`: the page's required controls",
        spec.id
    );
    assert_eq!(
        names(rows, "list"),
        sorted(&spec.lists.iter().map(|list| list.slot).collect::<Vec<_>>()),
        "`{}`: the page's lists",
        spec.id
    );
    for list in spec.lists {
        check_list(spec, list, rows);
    }

    let keys = names(rows, "copy key");
    assert_eq!(
        keys,
        sorted(spec.copy_keys),
        "`{}`: the page's copy keys",
        spec.id
    );
    for row in rows.iter().filter(|row| row.kind == "copy key") {
        let key = copy::key(&row.name)
            .unwrap_or_else(|| panic!("`{}`: the copy table has `{}`", spec.id, row.name));
        assert_eq!(
            row.placeholders, key.placeholders,
            "`{}`: the page's placeholders for `{}`",
            spec.id, row.name
        );
        assert_eq!(
            row.shows, key.text,
            "`{}`: the page's default text for `{}`",
            spec.id, row.name
        );
    }
}

fn check_list(spec: &SurfaceSpec, list: &ListSpec, rows: &[Row]) {
    assert_eq!(
        item_names(rows, "item slot", list.slot),
        sorted(list.item_slots),
        "`{}`: the page's item slots of `{}`",
        spec.id,
        list.slot
    );
    assert_eq!(
        item_names(rows, "optional item slot", list.slot),
        sorted(list.optional_item_slots),
        "`{}`: the page's optional item slots of `{}`",
        spec.id,
        list.slot
    );
    assert_eq!(
        item_names(rows, "item control", list.slot),
        sorted(list.item_controls),
        "`{}`: the page's item controls of `{}`",
        spec.id,
        list.slot
    );
}

/// Every surface's table on the page is the engine's table for it.
#[test]
fn the_page_lists_every_surface_s_interface() {
    let sections = page_sections();
    let ids: Vec<&str> = ui::SURFACES.iter().map(|spec| spec.id).collect();
    let mut listed: Vec<&str> = sections.keys().map(String::as_str).collect();
    listed.sort_unstable();
    assert_eq!(listed, sorted(&ids), "the page has one section per surface");
    for spec in ui::SURFACES {
        let rows = &sections[spec.id];
        assert!(!rows.is_empty(), "`{}` has a table", spec.id);
        for row in rows {
            assert!(
                matches!(
                    row.kind.as_str(),
                    "required slot"
                        | "optional slot"
                        | "required control"
                        | "list"
                        | "item slot"
                        | "optional item slot"
                        | "item control"
                        | "copy key"
                ),
                "`{}`: `{}` is not a row kind this page defines",
                spec.id,
                row.kind
            );
        }
        check_surface(spec, rows);
    }
}

/// The page names the five numbering styles the manifest accepts.
#[test]
fn the_page_names_every_numbering_style() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/ui-plugin-format.md");
    let page = std::fs::read_to_string(&path).expect("the format page reads");
    for style in Numbering::STYLES {
        assert!(
            page.contains(&format!("`\"{style}\"`")),
            "the page names the style `{style}`"
        );
    }
}
