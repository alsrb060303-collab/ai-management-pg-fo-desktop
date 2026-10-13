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
        let value = strip_comment(value.trim());

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
                .map(|item| unquote(strip_comment(item)))
                .collect::<Vec<_>>()
                .join(", ")
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            split_flow_list(inner)
                .into_iter()
                .map(|s| unquote(s.trim()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            let mut joined = unquote(value);
            // Plain scalars may wrap onto indented lines.
            if !value.starts_with(['"', '\'']) {
                for l in block
                    .iter()
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                {
                    joined.push(' ');
                    joined.push_str(strip_comment(l));
                }
            }
            joined
        };
        fields.insert(key, parsed);
    }
    Some(fields)
}

/// Splits a YAML flow list on commas outside quotes and braces, so a glob
/// such as `"src/**/*.{ts,tsx}"` stays one item.
fn split_flow_list(inner: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let (mut start, mut depth, mut quote) = (0, 0usize, None);
    for (i, c) in inner.char_indices() {
        match (quote, c) {
            (Some(q), _) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '{') => depth += 1,
            (None, '}') => depth = depth.saturating_sub(1),
            (None, ',') if depth == 0 => {
                items.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    items.push(&inner[start..]);
    items
}

/// Drops a trailing `# comment`. A quoted value ends at its closing quote; an
/// unquoted one at the first `#` preceded by whitespace.
fn strip_comment(s: &str) -> &str {
    let s = s.trim();
    if let Some(q) = s.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let mut escaped = false;
        let mut chars = s.char_indices().skip(1).peekable();
        while let Some((i, c)) = chars.next() {
            if q == '"' && c == '\\' && !escaped {
                escaped = true;
                continue;
            }
            if c == q && !escaped {
                // `''` inside single quotes is an escaped quote.
                if q == '\'' && chars.peek().is_some_and(|(_, n)| *n == '\'') {
                    chars.next();
                    continue;
                }
                return &s[..=i];
            }
            escaped = false;
        }
        return s;
    }
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'#' && i > 0 && bytes[i - 1].is_ascii_whitespace() {
            return s[..i].trim_end();
        }
    }
    s
}

/// Removes YAML quotes: `''` in single quotes is one `'`, and double quotes
/// take the common backslash escapes.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return s[1..s.len() - 1].replace("''", "'");
    }
    if !(s.len() >= 2 && s.starts_with('"') && s.ends_with('"')) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s[1..s.len() - 1].chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => {
                        out.push_str("\\u");
                        out.push_str(&hex);
                    }
                }
            }
            // `\"`, `\\`, `\/` and anything unknown: the character itself.
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
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
    fn flow_lists_keep_quoted_and_brace_globs_whole() {
        let f = parse("---\npaths: [\"src/**/*.{ts,tsx}\", 'a, b', lib/**]\n---\n").unwrap();
        assert_eq!(f["paths"], "src/**/*.{ts,tsx}, a, b, lib/**");
        let f = parse("---\npaths: [src/**/*.{ts,tsx}, lib/**]\n---\n").unwrap();
        assert_eq!(f["paths"], "src/**/*.{ts,tsx}, lib/**");
    }

    #[test]
    fn requires_frontmatter_on_the_first_line() {
        assert!(parse("\n---\nname: x\n---\n").is_none());
        assert!(parse("# Title\n").is_none());
        assert!(parse("---\nname: unterminated\n").is_none());
    }

    #[test]
    fn strips_comments() {
        let text = "---\nname: deploy # main one\ndescription: \"a # b\" # note\ntools: [Read, Grep] # x\npaths:\n  - src/** # ts\n---\n";
        let f = parse(text).unwrap();
        assert_eq!(f["name"], "deploy");
        assert_eq!(f["description"], "a # b");
        assert_eq!(f["tools"], "Read, Grep");
        assert_eq!(f["paths"], "src/**");
    }

    #[test]
    fn unescapes_quoted_values() {
        let f = parse("---\na: \"say \\\"hi\\\"\\n\\\\ \\u00e9\"\nb: 'it''s'\n---\n").unwrap();
        assert_eq!(f["a"], "say \"hi\"\n\\ é");
        assert_eq!(f["b"], "it's");
    }

    #[test]
    fn handles_bom_and_crlf() {
        let f = parse("\u{feff}---\r\nname: x\r\n---\r\n").unwrap();
        assert_eq!(f["name"], "x");
    }
}
