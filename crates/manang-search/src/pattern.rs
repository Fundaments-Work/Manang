//! Pattern compilation with hard resource limits, and glob filters.
//!
//! Every user-supplied regex goes through [`compile`]. The `regex` engine
//! behind `grep-regex` guarantees linear-time matching (no backtracking), so
//! the remaining denial-of-service surface is *compilation*: huge counted
//! repetitions and giant Unicode classes. Those are bounded by the compiled
//! size limit, the lazy-DFA cache limit, the nesting limit and a cap on the
//! pattern length.

use std::path::Path;

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};

use crate::error::SearchError;
use crate::limits::Limits;

/// What the user typed plus the toggles next to the search box.
#[derive(Debug, Clone)]
pub struct PatternSpec<'a> {
    pub query: &'a str,
    pub regex: bool,
    pub case_sensitive: bool,
    pub whole_word: bool,
}

/// Which line semantics the matcher is compiled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Line-oriented project search: the pattern may not match `\n`.
    Lines,
    /// In-document find over one big haystack; patterns may span lines.
    Document,
}

/// Compile `spec` into a matcher, enforcing [`Limits`].
pub fn compile(
    spec: &PatternSpec<'_>,
    mode: Mode,
    limits: &Limits,
) -> Result<RegexMatcher, SearchError> {
    if spec.query.is_empty() {
        return Err(SearchError::EmptyQuery);
    }
    if spec.query.len() > limits.max_pattern_bytes {
        return Err(SearchError::QueryTooLong {
            len: spec.query.len(),
            limit: limits.max_pattern_bytes,
        });
    }
    let mut builder = RegexMatcherBuilder::new();
    builder
        .case_insensitive(!spec.case_sensitive)
        .word(spec.whole_word)
        .fixed_strings(!spec.regex)
        // `^` / `$` anchor at line boundaries, as in every editor.
        .multi_line(true)
        .size_limit(limits.regex_size_limit)
        .dfa_size_limit(limits.regex_dfa_limit)
        .nest_limit(limits.regex_nest_limit);
    if mode == Mode::Lines {
        // Refuse patterns that could match a line terminator; `$` also
        // matches before `\r\n`.
        builder.line_terminator(Some(b'\n')).crlf(true);
    }
    builder
        .build(spec.query)
        .map_err(|e| SearchError::BadPattern(short(&e.to_string())))
}

/// Error text from the regex engine can quote the whole pattern; keep the UI
/// message small.
fn short(message: &str) -> String {
    const MAX: usize = 400;
    if message.len() <= MAX {
        return message.to_owned();
    }
    let mut end = MAX;
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &message[..end])
}

/// Compiled include/exclude filters.
#[derive(Debug, Clone, Default)]
pub struct PathFilter {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl PathFilter {
    pub fn new(
        include: &[String],
        exclude: &[String],
        limits: &Limits,
    ) -> Result<Self, SearchError> {
        let given = include.len() + exclude.len();
        if given > limits.max_globs {
            return Err(SearchError::TooManyGlobs {
                given,
                limit: limits.max_globs,
            });
        }
        Ok(Self {
            include: build_set(include)?,
            exclude: build_set(exclude)?,
        })
    }

    /// Does the project-relative, `/`-separated `rel` pass the filters?
    pub fn allows(&self, rel: &str) -> bool {
        let path = Path::new(rel);
        let included = self.include.as_ref().is_none_or(|s| s.is_match(path));
        let excluded = self.exclude.as_ref().is_some_and(|s| s.is_match(path));
        included && !excluded
    }
}

fn glob(pattern: &str) -> Result<Glob, SearchError> {
    GlobBuilder::new(pattern)
        .literal_separator(false)
        .backslash_escape(true)
        .build()
        .map_err(|e| SearchError::BadGlob {
            glob: pattern.chars().take(80).collect(),
            reason: e.kind().to_string(),
        })
}

/// Gitignore-flavoured expansion: a bare `*.rs` matches at any depth and a
/// directory name matches everything below it.
fn build_set(patterns: &[String]) -> Result<Option<GlobSet>, SearchError> {
    let mut set = GlobSetBuilder::new();
    let mut any = false;
    for raw in patterns {
        let p = raw.trim().trim_start_matches("./");
        if p.is_empty() {
            continue;
        }
        if p.len() > 512 {
            return Err(SearchError::BadGlob {
                glob: p.chars().take(80).collect(),
                reason: "glob is too long".to_owned(),
            });
        }
        let p = p.trim_end_matches('/');
        let mut forms = vec![p.to_owned(), format!("{p}/**")];
        if !p.contains('/') {
            forms.push(format!("**/{p}"));
            forms.push(format!("**/{p}/**"));
        }
        for form in forms {
            set.add(glob(&form)?);
        }
        any = true;
    }
    if !any {
        return Ok(None);
    }
    set.build().map(Some).map_err(|e| SearchError::BadGlob {
        glob: String::new(),
        reason: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(q: &str, regex: bool) -> PatternSpec<'_> {
        PatternSpec {
            query: q,
            regex,
            case_sensitive: false,
            whole_word: false,
        }
    }

    #[test]
    fn literal_is_escaped() {
        let m = compile(&spec("a.b(", false), Mode::Lines, &Limits::default());
        assert!(m.is_ok());
        let bad = compile(&spec("a.b(", true), Mode::Lines, &Limits::default());
        assert!(matches!(bad, Err(SearchError::BadPattern(_))));
    }

    #[test]
    fn rejects_empty_and_long() {
        let l = Limits::default();
        assert!(matches!(
            compile(&spec("", true), Mode::Lines, &l),
            Err(SearchError::EmptyQuery)
        ));
        let long = "a".repeat(l.max_pattern_bytes + 1);
        assert!(matches!(
            compile(&spec(&long, true), Mode::Lines, &l),
            Err(SearchError::QueryTooLong { .. })
        ));
    }

    #[test]
    fn pathological_sizes_are_refused() {
        let l = Limits::default();
        for p in [
            "((a{1000}){1000}){1000}",
            r"\p{L}{10000}",
            "(((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((a)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))",
        ] {
            assert!(
                compile(&spec(p, true), Mode::Lines, &l).is_err(),
                "{p} should be refused"
            );
        }
    }

    #[test]
    fn line_mode_refuses_newlines() {
        let l = Limits::default();
        assert!(compile(&spec(r"a\nb", true), Mode::Lines, &l).is_err());
        assert!(compile(&spec(r"a\nb", true), Mode::Document, &l).is_ok());
    }

    #[test]
    fn globs_are_gitignore_flavoured() {
        let l = Limits::default();
        let f = PathFilter::new(&["*.rs".into()], &["target".into(), "a/b.rs".into()], &l)
            .expect("filter");
        assert!(f.allows("src/main.rs"));
        assert!(f.allows("main.rs"));
        assert!(!f.allows("readme.md"));
        assert!(!f.allows("target/debug/x.rs"));
        assert!(!f.allows("a/b.rs"));
        assert!(PathFilter::default().allows("anything"));
    }

    #[test]
    fn bad_and_excessive_globs() {
        let l = Limits::default();
        assert!(PathFilter::new(&["[".into()], &[], &l).is_err());
        let many: Vec<String> = (0..l.max_globs + 1).map(|i| format!("{i}")).collect();
        assert!(matches!(
            PathFilter::new(&many, &[], &l),
            Err(SearchError::TooManyGlobs { .. })
        ));
    }
}
