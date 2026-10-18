//! The diagnostics report behind Help > Send feedback.
//!
//! Plain text the person reads before deciding where it goes: versions, the
//! system, the plugins and the tail of the log. Nothing is sent anywhere by
//! Manang. Home folders and anything that looks like a key or token are
//! replaced before the text leaves this module.

use std::fmt::Write as _;

/// How many log lines the report carries.
pub const LOG_TAIL_LINES: usize = 200;

/// Everything the report is made of.
pub struct ReportInput<'a> {
    pub version: &'a str,
    pub commit: &'a str,
    pub channel: &'a str,
    pub os: &'a str,
    pub arch: &'a str,
    pub webview: &'a str,
    /// `name version` per registered plugin.
    pub plugins: &'a [String],
    pub log: &'a str,
    /// The person's home folder, replaced by `~`.
    pub home: Option<&'a str>,
}

/// The last `count` lines of `text`.
pub fn tail_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

/// Prefixes after which a secret follows, and the shortest secret we mask.
const SECRET_PREFIXES: [&str; 6] = ["sk-", "ghp_", "github_pat_", "AKIA", "xoxb-", "xoxp-"];
const MIN_SECRET_LEN: usize = 16;

/// Replace the home folder with `~`, and anything key-like with `[redacted]`.
pub fn redact(text: &str, home: Option<&str>) -> String {
    let mut out = match home {
        Some(home) if !home.is_empty() && home != "/" => text.replace(home, "~"),
        _ => text.to_owned(),
    };
    out = redact_bearer(&out);
    for prefix in SECRET_PREFIXES {
        out = redact_prefixed(&out, prefix);
    }
    out
}

fn is_secret_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// `sk-abc...` (a prefix then enough token characters) becomes `[redacted]`.
fn redact_prefixed(text: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let token_len: usize = after
            .chars()
            .take_while(|c| is_secret_char(*c))
            .map(char::len_utf8)
            .sum();
        if token_len >= MIN_SECRET_LEN {
            out.push_str("[redacted]");
            rest = &after[token_len..];
        } else {
            out.push_str(prefix);
            rest = &after[prefix.len()..];
        }
    }
    out.push_str(rest);
    out
}

/// `Authorization: Bearer <token>` keeps the word and loses the token.
fn redact_bearer(text: &str) -> String {
    let marker = "Bearer ";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let end = at + marker.len();
        out.push_str(&rest[..end]);
        let after = &rest[end..];
        let token_len: usize = after
            .chars()
            .take_while(|c| is_secret_char(*c) || *c == '.' || *c == '~' || *c == '+' || *c == '/')
            .map(char::len_utf8)
            .sum();
        if token_len > 0 {
            out.push_str("[redacted]");
        }
        rest = &after[token_len..];
    }
    out.push_str(rest);
    out
}

/// The report text.
pub fn format_report(input: &ReportInput<'_>) -> String {
    let mut report = String::new();
    // Writing to a String cannot fail; the results are ignored on purpose.
    let _ = writeln!(report, "Manang {} ({})", input.version, input.channel);
    let _ = writeln!(report, "commit: {}", input.commit);
    let _ = writeln!(report, "system: {} {}", input.os, input.arch);
    let _ = writeln!(report, "web view: {}", input.webview);
    let _ = writeln!(report, "\nplugins ({}):", input.plugins.len());
    for plugin in input.plugins {
        let _ = writeln!(report, "  {plugin}");
    }
    let _ = writeln!(report, "\nlog (last {LOG_TAIL_LINES} lines):");
    report.push_str(&tail_lines(input.log, LOG_TAIL_LINES));
    report.push('\n');
    redact(&report, input.home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tail_keeps_the_last_lines() {
        assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail_lines("a\nb", 10), "a\nb");
        assert_eq!(tail_lines("", 3), "");
    }

    #[test]
    fn home_folders_become_a_tilde() {
        let text = "opened /home/ada/papers/main.typ and /home/ada/.config/x";
        assert_eq!(
            redact(text, Some("/home/ada")),
            "opened ~/papers/main.typ and ~/.config/x"
        );
        assert_eq!(redact(text, Some("/")), text);
        assert_eq!(redact(text, None), text);
    }

    #[test]
    fn keys_and_tokens_are_masked() {
        let text = "key sk-abcdefghijklmnopqrstuvwxyz0123 and ghp_0123456789abcdefABCDEF0123456789abcd end";
        assert_eq!(redact(text, None), "key [redacted] and [redacted] end");
        assert_eq!(
            redact("Authorization: Bearer abc.def-123_xyz", None),
            "Authorization: Bearer [redacted]"
        );
    }

    #[test]
    fn short_lookalikes_are_left_alone() {
        assert_eq!(redact("task-list and sk-1", None), "task-list and sk-1");
    }

    #[test]
    fn the_report_names_versions_plugins_and_redacts_the_log() {
        let plugins = vec!["typst 0.1.0".to_owned(), "git 0.1.0".to_owned()];
        let report = format_report(&ReportInput {
            version: "0.1.0",
            commit: "abc1234",
            channel: "beta",
            os: "linux",
            arch: "x86_64",
            webview: "WebKitGTK 2.52",
            plugins: &plugins,
            log: "line one\nopened /home/ada/x.typ\nBearer secrettoken123",
            home: Some("/home/ada"),
        });
        assert!(report.starts_with("Manang 0.1.0 (beta)\ncommit: abc1234\n"));
        assert!(report.contains("plugins (2):\n  typst 0.1.0\n  git 0.1.0"));
        assert!(report.contains("opened ~/x.typ"));
        assert!(report.contains("Bearer [redacted]"));
        assert!(!report.contains("/home/ada"));
        assert!(!report.contains("secrettoken123"));
    }
}
