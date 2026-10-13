//! Codex specifics of the library: converting subagents between the library's
//! Markdown and Codex TOML, and turning one skill off through `[[skills.config]]`
//! in the user's `config.toml` (docs/research.md section 7). Pure text and
//! explicit paths, so everything is testable without the app.

use std::fs;
use std::io;
use std::path::Path;

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use super::ops::{self, Result};
use crate::tools::frontmatter;

// ---------------------------------------------------------------------------
// Subagents: Markdown <-> TOML

/// Splits Markdown into its frontmatter fields and body. The body starts after
/// the closing `---` line; without frontmatter the whole text is the body.
fn split_markdown(text: &str) -> (std::collections::BTreeMap<String, String>, String) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(fields) = frontmatter::parse(text) else {
        return (Default::default(), text.to_string());
    };
    // `parse` succeeded, so the first line is `---` and a closing one follows.
    let mut lines = text.split_inclusive('\n');
    let mut offset = lines.next().map_or(0, str::len);
    for line in lines {
        offset += line.len();
        if line.trim_end() == "---" {
            break;
        }
    }
    (fields, text[offset..].to_string())
}

fn required<'a>(
    fields: &'a std::collections::BTreeMap<String, String>,
    key: &str,
) -> Option<&'a str> {
    fields.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

/// Converts a library subagent (Markdown with `name` and `description`
/// frontmatter) into a Codex agent file: `name`, `description` and the body as
/// `developer_instructions`. Claude-only fields (tools, model, ...) are
/// dropped; Claude model names mean nothing to Codex.
pub fn agent_to_toml(markdown: &str) -> Result<String> {
    let (fields, body) = split_markdown(markdown);
    let (Some(name), Some(description)) =
        (required(&fields, "name"), required(&fields, "description"))
    else {
        return Err(crate::tr!(
            "Codex용으로 바꾸려면 subagent의 frontmatter에 name과 description이 있어야 합니다",
            "To convert it for Codex, the subagent needs name and description in its frontmatter"
        ));
    };
    let body = body.replace("\r\n", "\n");
    let body = body.trim_start_matches('\n').trim_end();
    let mut doc = DocumentMut::new();
    doc["name"] = value(name);
    doc["description"] = value(description);
    doc["developer_instructions"] = value(body);
    Ok(doc.to_string())
}

/// Converts a Codex agent file back into a library subagent: `name` and
/// `description` frontmatter, `developer_instructions` as the body. Other
/// Codex settings (model, sandbox, ...) have no Markdown form and are dropped.
pub fn agent_from_toml(toml: &str) -> Result<String> {
    let doc: DocumentMut = toml.parse().map_err(|e| {
        crate::tr!(
            "TOML을 읽을 수 없습니다: {e}",
            "Couldn't read the TOML: {e}"
        )
    })?;
    let get = |key: &str| {
        doc.get(key)
            .and_then(Item::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let (Some(name), Some(description)) = (get("name"), get("description")) else {
        return Err(crate::tr!(
            "Codex agent 파일에 name과 description이 있어야 합니다",
            "The Codex agent file needs a name and a description"
        ));
    };
    let body = doc
        .get("developer_instructions")
        .and_then(Item::as_str)
        .unwrap_or_default()
        .trim_end();
    let mut out = format!(
        "---\nname: {}\ndescription: {}\n---\n",
        yaml_scalar(name),
        yaml_scalar(description)
    );
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        out.push('\n');
    }
    Ok(out)
}

/// A YAML scalar for `s`: plain when that reads back unchanged, otherwise
/// double-quoted (a JSON string is a valid YAML double-quoted scalar).
fn yaml_scalar(s: &str) -> String {
    let plain = !s.is_empty()
        && s.trim() == s
        && !s.starts_with([
            '-', '?', ':', ',', '[', ']', '{', '}', '#', '&', '*', '!', '|', '>', '\'', '"', '%',
            '@', '`',
        ])
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.ends_with(':')
        && !s.chars().any(char::is_control)
        && !matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "null" | "~"
        )
        && s.parse::<f64>().is_err();
    if plain {
        s.to_string()
    } else {
        serde_json::to_string(s).unwrap_or_else(|_| format!("\"{s}\""))
    }
}

// ---------------------------------------------------------------------------
// Turning a skill off: `[[skills.config]]` in `$CODEX_HOME/config.toml`

/// Reads `config.toml`; a missing or empty file is an empty document. A file
/// that does not parse is an error, so it is never overwritten.
fn read_config(file: &Path) -> Result<DocumentMut> {
    let text = match fs::read(file) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(crate::tr!(
                "읽기 실패 ({}): {e}",
                "Couldn't read {}: {e}",
                file.display()
            ));
        }
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    text.parse().map_err(|_| {
        crate::tr!(
            "{}을(를) 읽을 수 없어 바꾸지 않았습니다 (TOML 오류)",
            "Left {} unchanged because it couldn't be read (TOML error)",
            file.display()
        )
    })
}

fn not_a_table(file: &Path) -> String {
    crate::tr!(
        "{}의 skills.config 형식을 알 수 없어 바꾸지 않았습니다",
        "Left {} unchanged because its skills.config has an unexpected form",
        file.display()
    )
}

/// The `[[skills.config]]` entries, or `None` when there are none.
fn entries<'a>(doc: &'a DocumentMut, file: &Path) -> Result<Option<&'a ArrayOfTables>> {
    let Some(skills) = doc.get("skills") else {
        return Ok(None);
    };
    let Some(config) = skills.get("config") else {
        return Ok(None);
    };
    config
        .as_array_of_tables()
        .map(Some)
        .ok_or_else(|| not_a_table(file))
}

fn is_entry(table: &Table, path: &str, enabled: bool) -> bool {
    table.get("path").and_then(Item::as_str) == Some(path)
        && table.get("enabled").and_then(Item::as_bool) == Some(enabled)
}

/// Whether the skill at `path` is currently off: the last entry for it (later
/// entries override earlier ones) has `enabled = false`.
pub fn skill_disabled(file: &Path, path: &str) -> Result<bool> {
    let doc = read_config(file)?;
    Ok(entries(&doc, file)?.is_some_and(|list| {
        list.iter()
            .filter(|t| t.get("path").and_then(Item::as_str) == Some(path))
            .last()
            .is_some_and(|t| t.get("enabled").and_then(Item::as_bool) == Some(false))
    }))
}

/// Appends `[[skills.config]] path = "<path>"`, `enabled = false`, keeping the
/// rest of the file (comments, order, formatting) as it is.
pub fn skill_off(file: &Path, path: &str) -> Result<()> {
    let mut doc = read_config(file)?;
    let root = doc.as_table_mut();
    if !root.contains_key("skills") {
        let mut skills = Table::new();
        // Only `[[skills.config]]` headers, no empty `[skills]` one.
        skills.set_implicit(true);
        root.insert("skills", Item::Table(skills));
    }
    let skills = root
        .get_mut("skills")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| not_a_table(file))?;
    let list = skills
        .entry("config")
        .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or_else(|| not_a_table(file))?;
    let mut entry = Table::new();
    entry["path"] = value(path);
    entry["enabled"] = value(false);
    list.push(entry);
    ops::write_atomic(file, doc.to_string())
}

/// Removes the entry `skill_off` added for `path`. If the user changed it
/// (e.g. to `enabled = true`), nothing is removed. Returns whether the file
/// was written.
pub fn skill_on(file: &Path, path: &str) -> Result<bool> {
    if !file.exists() {
        return Ok(false);
    }
    let mut doc = read_config(file)?;
    let Some(index) = entries(&doc, file)?.and_then(|list| {
        list.iter()
            .enumerate()
            .filter(|(_, t)| is_entry(t, path, false))
            .map(|(i, _)| i)
            .last()
    }) else {
        return Ok(false);
    };
    let root = doc.as_table_mut();
    let Some(skills) = root.get_mut("skills").and_then(Item::as_table_mut) else {
        return Ok(false);
    };
    let emptied = match skills
        .get_mut("config")
        .and_then(Item::as_array_of_tables_mut)
    {
        Some(list) => {
            list.remove(index);
            list.is_empty()
        }
        None => return Ok(false),
    };
    if emptied {
        skills.remove("config");
    }
    if skills.is_empty() && skills.is_implicit() {
        root.remove("skills");
    }
    ops::write_atomic(file, doc.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml: &str) -> DocumentMut {
        toml.parse().unwrap()
    }

    #[test]
    fn agent_converts_to_toml_dropping_claude_fields() {
        let md = "---\nname: reviewer\ndescription: \"Reviews: \\\"carefully\\\"\"\ntools: Read, Grep\nmodel: opus\n---\n\nLine one with \"quotes\" and 'single'.\nA \\ backslash.\n\"\"\" triple inside\n";
        let toml = agent_to_toml(md).unwrap();
        let doc = parse(&toml);
        assert_eq!(doc["name"].as_str(), Some("reviewer"));
        assert_eq!(doc["description"].as_str(), Some("Reviews: \"carefully\""));
        assert_eq!(
            doc["developer_instructions"].as_str(),
            Some("Line one with \"quotes\" and 'single'.\nA \\ backslash.\n\"\"\" triple inside")
        );
        assert!(doc.get("tools").is_none() && doc.get("model").is_none());
        assert!(!toml.contains("opus"));
    }

    #[test]
    fn multi_line_body_reads_as_a_multi_line_string() {
        let md = "---\r\nname: a\r\ndescription: b\r\n---\r\nfirst\r\n\r\nsecond\r\n";
        let toml = agent_to_toml(md).unwrap();
        assert!(toml.contains("\"\"\"") || toml.contains("'''"), "{toml}");
        assert_eq!(
            parse(&toml)["developer_instructions"].as_str(),
            Some("first\n\nsecond")
        );
    }

    #[test]
    fn agent_without_name_or_description_is_refused() {
        assert!(agent_to_toml("---\nname: a\n---\nbody").is_err());
        assert!(agent_to_toml("---\ndescription: b\n---\nbody").is_err());
        assert!(agent_to_toml("no frontmatter").is_err());
        assert!(agent_from_toml("name = \"a\"\ndeveloper_instructions = \"x\"").is_err());
        assert!(agent_from_toml("not = [valid").is_err());
    }

    #[test]
    fn agent_round_trips_through_toml() {
        let toml = "name = \"lint: strict\"\ndescription = 'Says \"no\" # often'\nmodel = \"gpt-5\"\ndeveloper_instructions = \"\"\"\nStep 1\n  indented\nEnd with \\\"\\\"\\\" quotes\"\"\"\n";
        let md = agent_from_toml(toml).unwrap();
        let fields = frontmatter::parse(&md).unwrap();
        assert_eq!(fields["name"], "lint: strict");
        assert_eq!(fields["description"], "Says \"no\" # often");
        assert!(!md.contains("gpt-5"));
        let back = parse(&agent_to_toml(&md).unwrap());
        assert_eq!(back["name"].as_str(), Some("lint: strict"));
        assert_eq!(back["description"].as_str(), Some("Says \"no\" # often"));
        assert_eq!(
            back["developer_instructions"].as_str(),
            Some("Step 1\n  indented\nEnd with \"\"\" quotes")
        );
    }

    #[test]
    fn yaml_scalars_quote_only_when_needed() {
        assert_eq!(yaml_scalar("deploy"), "deploy");
        assert_eq!(yaml_scalar("Ship it now"), "Ship it now");
        assert_eq!(yaml_scalar("a: b"), "\"a: b\"");
        assert_eq!(yaml_scalar("yes"), "\"yes\"");
        assert_eq!(yaml_scalar("1.5"), "\"1.5\"");
        assert_eq!(yaml_scalar("two\nlines"), "\"two\\nlines\"");
    }

    #[test]
    fn skill_off_and_on_keep_other_content() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("config.toml");
        let original = "# my settings\nmodel = \"gpt-5\"  # favourite\n\n[mcp_servers.docs]\ncommand = \"npx\"\n\n[[skills.config]]\npath = \"/other/SKILL.md\"\nenabled = false # mine\n";
        fs::write(&file, original).unwrap();
        let skill = "/repo/.agents/skills/deploy/SKILL.md";

        assert!(!skill_disabled(&file, skill).unwrap());
        skill_off(&file, skill).unwrap();
        assert!(skill_disabled(&file, skill).unwrap());
        let text = fs::read_to_string(&file).unwrap();
        assert!(text.starts_with(original), "{text}");
        assert_eq!(
            parse(&text)["skills"]["config"]
                .as_array_of_tables()
                .unwrap()
                .len(),
            2
        );

        assert!(skill_on(&file, skill).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
        assert!(!skill_on(&file, skill).unwrap(), "nothing of ours left");
    }

    #[test]
    fn skill_off_creates_the_file_and_on_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("codex").join("config.toml");
        let skill = r"C:\repo\.agents\skills\a b\SKILL.md";
        skill_off(&file, skill).unwrap();
        let text = fs::read_to_string(&file).unwrap();
        assert!(!text.contains("[skills]\n"), "{text}");
        assert!(text.contains("[[skills.config]]"));
        assert!(skill_disabled(&file, skill).unwrap());
        skill_on(&file, skill).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap().trim(), "");
    }

    #[test]
    fn skill_on_leaves_an_entry_the_user_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("config.toml");
        let skill = "/repo/.agents/skills/deploy/SKILL.md";
        skill_off(&file, skill).unwrap();
        let edited = fs::read_to_string(&file)
            .unwrap()
            .replace("enabled = false", "enabled = true");
        fs::write(&file, &edited).unwrap();
        assert!(!skill_on(&file, skill).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), edited);
    }

    #[test]
    fn broken_or_unexpected_config_is_not_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("config.toml");
        fs::write(&file, "model = [broken").unwrap();
        assert!(skill_off(&file, "/a/SKILL.md").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "model = [broken");

        fs::write(&file, "skills = \"x\"\n").unwrap();
        assert!(skill_off(&file, "/a/SKILL.md").is_err());
        assert_eq!(fs::read_to_string(&file).unwrap(), "skills = \"x\"\n");
    }
}
