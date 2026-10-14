//! Prompt text as it is stored and synced: shortened, whitespace tidied, and
//! anything that looks like a key or token masked. The full prompt stays only
//! in the tool's own session file.

use std::sync::OnceLock;

use regex::{Captures, Regex};

pub const MASK: &str = "[가림]";
const MAX_CHARS: usize = 500;

struct Patterns {
    private_key: Regex,
    jwt: Regex,
    token: Regex,
    bearer: Regex,
    assignment: Regex,
    long_run: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| {
        let re = |s: &str| Regex::new(s).expect("valid pattern");
        Patterns {
            // An unterminated block (cut short) is masked to the end.
            private_key: re(
                r"-----BEGIN [A-Z ]*PRIVATE KEY-----(?:[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----|[\s\S]*)",
            ),
            jwt: re(r"eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]*"),
            token: re(concat!(
                r"github_pat_[A-Za-z0-9_]{20,}",
                r"|gh[pousr]_[A-Za-z0-9]{20,}",
                r"|sk-ant-[A-Za-z0-9_-]{10,}",
                r"|sk-[A-Za-z0-9_-]{16,}",
                r"|AKIA[0-9A-Z]{16}",
                r"|xox[baprs]-[A-Za-z0-9-]{10,}",
            )),
            bearer: re(r"(?i)(\bBearer[ \t]+)[A-Za-z0-9._~+/=-]{8,}"),
            assignment: re(
                r#"(?i)((?:password|passwd|token|secret|api[_-]?key)[ \t]*=[ \t]*|api[_-]?key[ \t]*:[ \t]*)(["']?)[^\s"',;]+"#,
            ),
            long_run: re(r"[A-Za-z0-9_\-+/=]{32,}"),
        }
    })
}

/// Trims, collapses runs of spaces and tabs (at most one blank line is kept),
/// masks secrets and cuts to 500 characters.
pub fn prompt_text(raw: &str) -> String {
    let p = patterns();
    let tidy = tidy(raw);
    let text = p.private_key.replace_all(&tidy, MASK);
    let text = p.jwt.replace_all(&text, MASK);
    let text = p.token.replace_all(&text, MASK);
    let text = p
        .bearer
        .replace_all(&text, |c: &Captures| format!("{}{MASK}", &c[1]));
    let text = p
        .assignment
        .replace_all(&text, |c: &Captures| format!("{}{}{MASK}", &c[1], &c[2]));
    let text = p.long_run.replace_all(&text, |c: &Captures| {
        let run = &c[0];
        if run.chars().any(|ch| ch.is_ascii_alphabetic())
            && run.chars().any(|ch| ch.is_ascii_digit())
        {
            MASK.to_string()
        } else {
            run.to_string()
        }
    });
    cut(&text)
}

fn tidy(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(normalized.len());
    let mut blank = 0;
    for line in normalized.lines() {
        let mut collapsed = String::new();
        let mut space = false;
        for ch in line.chars() {
            if ch == ' ' || ch == '\t' {
                space = true;
            } else {
                if space && !collapsed.is_empty() {
                    collapsed.push(' ');
                }
                space = false;
                collapsed.push(ch);
            }
        }
        if collapsed.is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if blank > 0 {
                out.push('\n');
            }
        }
        blank = 0;
        out.push_str(&collapsed);
    }
    out
}

fn cut(text: &str) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    let mut out: String = text.chars().take(MAX_CHARS - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidies_whitespace() {
        assert_eq!(prompt_text("  a \t  b  \r\n\r\n\r\n\r\n c\t"), "a b\n\nc");
        assert_eq!(prompt_text("a\nb"), "a\nb");
        assert_eq!(prompt_text("   "), "");
    }

    #[test]
    fn masks_api_keys() {
        let out =
            prompt_text("use sk-abcdefghijklmnopqrstuvwx and sk-ant-api03-abcDEF_123-xyz now");
        assert_eq!(out, format!("use {MASK} and {MASK} now"));
        assert_eq!(
            prompt_text("sk-proj-abcdefghijklmnop1234"),
            MASK,
            "hyphenated project keys"
        );
        assert_eq!(prompt_text("sk-short"), "sk-short");
    }

    #[test]
    fn masks_github_and_cloud_tokens() {
        for t in [
            "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "gho_abcdefghijklmnopqrstuvwxyz0123456789",
            "ghu_abcdefghijklmnopqrstuvwxyz0123456789",
            "ghs_abcdefghijklmnopqrstuvwxyz0123456789",
            "ghr_abcdefghijklmnopqrstuvwxyz0123456789",
            "github_pat_11ABCDEFG0abcdefghij_klmnopqrstuvwxyz",
            "AKIAIOSFODNN7EXAMPLE",
            "xoxb-123456789012-abcdefghij",
            "xoxp-123456789012-abcdefghij",
        ] {
            assert_eq!(
                prompt_text(&format!("x {t} y")),
                format!("x {MASK} y"),
                "{t}"
            );
        }
    }

    #[test]
    fn masks_private_key_blocks() {
        let pem = "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nabc\n-----END RSA PRIVATE KEY-----\nafter";
        assert_eq!(prompt_text(pem), format!("before\n{MASK}\nafter"));
        assert_eq!(
            prompt_text("-----BEGIN PRIVATE KEY-----\nMIIE"),
            MASK,
            "unterminated"
        );
    }

    #[test]
    fn masks_bearer_and_assignments() {
        assert_eq!(
            prompt_text("curl -H 'Authorization: Bearer abc.def-123_xyz' u"),
            format!("curl -H 'Authorization: Bearer {MASK}' u")
        );
        assert_eq!(
            prompt_text("password=hunter2 ok"),
            format!("password={MASK} ok")
        );
        assert_eq!(
            prompt_text("DB_TOKEN = abc123"),
            format!("DB_TOKEN = {MASK}")
        );
        assert_eq!(
            prompt_text("secret=\"s3cr3t\""),
            format!("secret=\"{MASK}\"")
        );
        assert_eq!(prompt_text("api_key=zzz"), format!("api_key={MASK}"));
        assert_eq!(prompt_text("apikey: zzz"), format!("apikey: {MASK}"));
        assert_eq!(
            prompt_text("the token: how it works"),
            "the token: how it works",
            "plain prose is kept"
        );
    }

    #[test]
    fn masks_jwts() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.abc-DEF_ghi";
        assert_eq!(prompt_text(&format!("t {jwt}")), format!("t {MASK}"));
    }

    #[test]
    fn masks_long_mixed_runs_only() {
        let secret = "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6";
        assert_eq!(secret.len(), 32);
        assert_eq!(prompt_text(secret), MASK);
        // Letters only, digits only, or shorter: kept.
        let letters = "abcdefghijklmnopqrstuvwxyzabcdefgh";
        assert_eq!(prompt_text(letters), letters);
        let digits = "12345678901234567890123456789012345";
        assert_eq!(prompt_text(digits), digits);
        assert_eq!(prompt_text("a1B2c3D4e5F6g7H8"), "a1B2c3D4e5F6g7H8");
    }

    #[test]
    fn keeps_korean_and_cuts_on_char_boundaries() {
        let ko = "로그인 화면에서 비밀번호 찾기 버튼이 안 눌려요. src/login.tsx 확인해 줘";
        assert_eq!(prompt_text(ko), ko);

        let long = "가".repeat(700);
        let out = prompt_text(&long);
        assert_eq!(out.chars().count(), 500);
        assert!(out.ends_with('…'));
        let exact = "나".repeat(500);
        assert_eq!(prompt_text(&exact), exact);
    }
}
