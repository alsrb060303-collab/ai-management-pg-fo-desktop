//! Minimal YAML frontmatter reader for SKILL.md, subagent and rule files.
//!
//! Only top-level `key: value` pairs are read: plain or quoted scalars, inline
//! lists (`[a, b]`), block lists (`- a`) and block scalars (`|`, `>`). That covers
//! the fields we display; anything fancier is ignored rather than rejected, as
//! Claude Code itself ignores unknown fields.

use std::collections::BTreeMap;

/// Returns the frontmatter fields, or `None` when the file has no frontmatter.
/// Like Claude Code, frontmatter only counts when `---` is the very first line.
pub fn parse(text: &str) -> Option<BTreeMap<String, String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }

    let mut body = Vec::new();
    let mut closed = false;
    for line in lines {
        if line.trim_end() == "---" {
            closed = true;
            break;
        }
        body.push(line);
    }
    if !closed {
        return None;
    }

    let mut fields = BTreeMap::new();
    let mut i = 0;
    while i < body.len() {
        let line = body[i];
        i += 1;
        if line.starts_with([' ', '\t']) || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = value.trim();

        // Collect the indented continuation lines that belong to this key.
        let start = i;
        while i < body.len() && (body[i].starts_with([' ', '\t']) || body[i].trim().is_empty()) {
            i += 1;
        }
        let block: Vec<&str> = body[start..i].iter().map(|l| l.trim()).collect();

        let parsed = if value.starts_with('|') || value.starts_with('>') {
            let sep = if value.starts_with('|') { "\n" } else { " " };
            block
                .iter()
                .copied()
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(sep)
        } else if value.is_empty() {
            block
                .iter()
                .filter_map(|l| l.strip_prefix("- "))
                .map(unquote)
                .collect::<Vec<_>>()
                .join(", ")
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            inner
                .split(',')
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            let mut joined = unquote(value);
            // Plain scalars may wrap onto indented lines.
            for l in block.iter().filter(|l| !l.is_empty()) {
                joined.push(' ');
                joined.push_str(l);
            }
            joined
        };
        fields.insert(key, parsed);
    }
    Some(fields)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_and_quoted_values() {
        let f = parse("---\nname: deploy\ndescription: \"Ship it: now\"\n---\nbody").unwrap();
        assert_eq!(f["name"], "deploy");
        assert_eq!(f["description"], "Ship it: now");
    }

    #[test]
    fn reads_block_scalars_and_lists() {
        let text = "---\ndescription: >\n  first line\n  second line\npaths:\n  - src/**/*.ts\n  - lib/**\ntools: [Read, Grep]\n---\n";
        let f = parse(text).unwrap();
        assert_eq!(f["description"], "first line second line");
        assert_eq!(f["paths"], "src/**/*.ts, lib/**");
        assert_eq!(f["tools"], "Read, Grep");
    }

    #[test]
    fn requires_frontmatter_on_the_first_line() {
        assert!(parse("\n---\nname: x\n---\n").is_none());
        assert!(parse("# Title\n").is_none());
        assert!(parse("---\nname: unterminated\n").is_none());
    }

    #[test]
    fn handles_bom_and_crlf() {
        let f = parse("\u{feff}---\r\nname: x\r\n---\r\n").unwrap();
        assert_eq!(f["name"], "x");
    }
}
