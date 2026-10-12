//! Parser and lookup table for translation catalogs (`*.tsv`).
//! Standard Crafting Apps catalog format.

use std::collections::{HashMap, HashSet};

/// A catalog entry as read from the file.
#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    pub context: String,
    pub source: String,
    pub translation: String,
}

/// The contexts a catalog entry may name.
pub const CONTEXTS: &[&str] = &["preview", "menu", "tooltip"];

/// A parsed catalog.
#[derive(Debug, Default)]
pub struct Catalog {
    plain: HashMap<String, String>,
    contextual: HashMap<(String, String), String>,
}

fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => return Err(format!("unknown escape \\{other}; use \\n, \\t or \\\\")),
            None => return Err("trailing backslash".into()),
        }
    }
    Ok(out)
}

pub fn placeholders(text: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some((_, after_open)) = rest.split_once('{') {
        let Some((name, after_close)) = after_open.split_once('}') else { break };
        names.push(name);
        rest = after_close;
    }
    names.sort_unstable();
    names
}

fn validate(entry: &Entry) -> Result<(), String> {
    let Entry { context, source, translation } = entry;
    if !context.is_empty() && !CONTEXTS.contains(&context.as_str()) {
        return Err(format!("unknown context {context:?}"));
    }
    if placeholders(source) != placeholders(translation) {
        return Err("placeholders differ from the English source".into());
    }
    if source.ends_with('…') != translation.ends_with('…') {
        return Err("trailing ellipsis differs from the English source".into());
    }
    Ok(())
}

fn parse_line(line: &str) -> Result<Entry, String> {
    let mut columns = line.split('\t');
    let (Some(context), Some(source), Some(translation), None) = (columns.next(), columns.next(), columns.next(), columns.next()) else {
        return Err("expected `context<TAB>source<TAB>translation`".into());
    };
    let entry = Entry { context: unescape(context)?, source: unescape(source)?, translation: unescape(translation)? };
    if entry.source.is_empty() || entry.translation.is_empty() {
        return Err("source and translation must be nonempty".into());
    }
    validate(&entry)?;
    Ok(entry)
}

pub fn parse_entries(text: &str) -> (Vec<Entry>, Vec<String>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let line_no = index.saturating_add(1);
        match parse_line(line) {
            Ok(entry) if !seen.insert((entry.context.clone(), entry.source.clone())) => {
                errors.push(format!("line {line_no}: duplicate key {:?} / {:?}", entry.context, entry.source));
            }
            Ok(entry) => entries.push(entry),
            Err(error) => errors.push(format!("line {line_no}: {error}")),
        }
    }
    (entries, errors)
}

impl Catalog {
    pub fn parse(text: &str) -> (Catalog, Vec<String>) {
        let (entries, errors) = parse_entries(text);
        let mut catalog = Catalog::default();
        for e in entries {
            if e.context.is_empty() {
                catalog.plain.insert(e.source, e.translation);
            } else {
                catalog.contextual.insert((e.context, e.source), e.translation);
            }
        }
        (catalog, errors)
    }

    pub fn plain(&self, s: &str) -> Option<&str> {
        self.plain.get(s).map(String::as_str)
    }

    pub fn in_context(&self, context: &str, s: &str) -> Option<&str> {
        self.contextual.get(&(context.to_string(), s.to_string())).map(String::as_str).or_else(|| self.plain(s))
    }
}
