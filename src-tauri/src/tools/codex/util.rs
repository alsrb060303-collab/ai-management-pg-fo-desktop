//! File and TOML helpers shared by the Codex scanner.

use std::fs;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, TableLike};

/// Reads a text file as UTF-8 (lossy) without a leading BOM.
pub fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

/// `None` when the file is missing, otherwise the parsed document or a
/// one-line error.
pub fn read_toml(path: &Path) -> Option<Result<DocumentMut, String>> {
    if !path.is_file() {
        return None;
    }
    let text = read_text(path)?;
    Some(
        text.parse::<DocumentMut>()
            .map_err(|e| toml_error(&e.to_string())),
    )
}

/// toml_edit errors span several lines (position, source excerpt, message);
/// keep the position and the message.
fn toml_error(message: &str) -> String {
    let lines: Vec<&str> = message.lines().map(str::trim).collect();
    let first = lines.first().copied().unwrap_or_default();
    let last = lines
        .iter()
        .rev()
        .find(|l| !l.is_empty() && !l.contains('|'))
        .copied()
        .unwrap_or_default();
    if last.is_empty() || last == first {
        first.to_string()
    } else {
        format!("{first}: {last}")
    }
}

/// `item[a][b]...`, if every key exists.
pub fn get<'a>(item: &'a Item, keys: &[&str]) -> Option<&'a Item> {
    keys.iter()
        .try_fold(item, |item, key| item.get(*key))
        .filter(|i| !i.is_none())
}

pub fn get_str<'a>(item: &'a Item, keys: &[&str]) -> Option<&'a str> {
    get(item, keys)?.as_str()
}

pub fn get_bool(item: &Item, keys: &[&str]) -> Option<bool> {
    get(item, keys)?.as_bool()
}

/// An array of strings (other values in it are skipped).
pub fn get_strings(item: &Item, keys: &[&str]) -> Option<Vec<String>> {
    let array = get(item, keys)?.as_array()?;
    Some(
        array
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

/// A non-blank string field of a table.
pub fn field<'a>(table: &'a dyn TableLike, key: &str) -> Option<&'a str> {
    table
        .get(key)
        .and_then(Item::as_str)
        .filter(|s| !s.trim().is_empty())
}

/// `[[x]]` tables or an inline `x = [{...}]` array, as tables.
pub fn table_list(item: &Item) -> Vec<&dyn TableLike> {
    if let Some(tables) = item.as_array_of_tables() {
        tables.iter().map(|t| t as &dyn TableLike).collect()
    } else if let Some(array) = item.as_array() {
        array
            .iter()
            .filter_map(|v| v.as_inline_table().map(|t| t as &dyn TableLike))
            .collect()
    } else {
        Vec::new()
    }
}

/// Path compared the way Codex compares project paths: resolved, and on
/// Windows without the `\\?\` prefix and lowercased.
pub fn norm(path: &Path) -> PathBuf {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if cfg!(windows) {
        let s = path.to_string_lossy();
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
        PathBuf::from(s.to_ascii_lowercase())
    } else {
        path
    }
}

pub fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub fn first_heading(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("# "))
        .map(|h| h.trim().to_string())
}

/// `<root>/<name>/SKILL.md` for each non-hidden folder, in a stable order.
pub fn skill_files(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir() && !is_hidden(p))
        .collect();
    dirs.sort();
    dirs.into_iter()
        .map(|dir| dir.join("SKILL.md"))
        .filter(|p| p.is_file())
        .collect()
}

/// Files directly in `dir` with extension `ext`, in a stable order.
pub fn files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && !is_hidden(p) && p.extension().is_some_and(|x| x == ext))
        .collect();
    files.sort();
    files
}
