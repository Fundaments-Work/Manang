//! A small prose-lint rule engine.
//!
//! Deliberately not Vale: a handful of fast, deterministic heuristics over
//! already-masked prose ([`crate::mask::prose`]), so nothing inside code, math
//! or comments is ever flagged. Each rule can be switched off, re-graded
//! (`error`/`warning`/`info`) and, where it makes sense, tuned.

use std::collections::HashMap;

use serde_json::Value;

use crate::diag::{DiagSeverity, Diagnostic, MAX_DIAGNOSTICS};
use crate::error::WritingError;
use crate::mask::OPAQUE;
use crate::text::{CharRange, TextIndex};
use crate::words::{Word, sentences, words};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Rule {
    RepeatedWord,
    LongSentence,
    PassiveVoice,
    WeaselWords,
    DoubleSpace,
    Hyphenation,
}

impl Rule {
    pub const ALL: [Rule; 6] = [
        Rule::RepeatedWord,
        Rule::LongSentence,
        Rule::PassiveVoice,
        Rule::WeaselWords,
        Rule::DoubleSpace,
        Rule::Hyphenation,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Rule::RepeatedWord => "repeated-word",
            Rule::LongSentence => "long-sentence",
            Rule::PassiveVoice => "passive-voice",
            Rule::WeaselWords => "weasel-words",
            Rule::DoubleSpace => "double-space",
            Rule::Hyphenation => "hyphenation",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }

    fn default_severity(self) -> DiagSeverity {
        match self {
            Rule::RepeatedWord => DiagSeverity::Warning,
            _ => DiagSeverity::Info,
        }
    }
}

/// Settings of one rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleConfig {
    pub enabled: bool,
    pub severity: DiagSeverity,
    /// `long-sentence`: report sentences with more words than this.
    pub max_words: usize,
    /// `weasel-words`: additional single words to flag.
    pub extra_words: Vec<String>,
    /// `repeated-word`/`weasel-words`: words never to flag.
    pub ignore_words: Vec<String>,
}

impl RuleConfig {
    fn new(rule: Rule) -> Self {
        Self {
            enabled: true,
            severity: rule.default_severity(),
            max_words: 40,
            extra_words: Vec::new(),
            ignore_words: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintConfig {
    rules: HashMap<Rule, RuleConfig>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            rules: Rule::ALL
                .into_iter()
                .map(|r| (r, RuleConfig::new(r)))
                .collect(),
        }
    }
}

impl LintConfig {
    /// Parse the `rules` argument.
    ///
    /// * absent or `null`: every rule on with defaults;
    /// * an array of rule ids: only those rules on;
    /// * an object mapping rule id to `false`/`true` or to
    ///   `{enabled, severity, max_words, extra_words, ignore_words}`; rules
    ///   not mentioned keep their defaults.
    pub fn from_json(value: Option<&Value>) -> Result<Self, WritingError> {
        let mut config = Self::default();
        match value {
            None | Some(Value::Null) => {}
            Some(Value::Array(ids)) => {
                for rule in config.rules.values_mut() {
                    rule.enabled = false;
                }
                for id in ids {
                    let rule = rule_from(id.as_str().unwrap_or(""))?;
                    config.rule_mut(rule).enabled = true;
                }
            }
            Some(Value::Object(map)) => {
                for (id, setting) in map {
                    let rule = rule_from(id)?;
                    config.apply(rule, setting)?;
                }
            }
            Some(_) => {
                return Err(WritingError::bad_arg(
                    "rules",
                    "expected an array of rule ids or an object of settings",
                ));
            }
        }
        Ok(config)
    }

    fn rule_mut(&mut self, rule: Rule) -> &mut RuleConfig {
        self.rules
            .entry(rule)
            .or_insert_with(|| RuleConfig::new(rule))
    }

    fn apply(&mut self, rule: Rule, setting: &Value) -> Result<(), WritingError> {
        let cfg = self.rule_mut(rule);
        match setting {
            Value::Bool(on) => cfg.enabled = *on,
            Value::Object(map) => {
                for (key, v) in map {
                    match key.as_str() {
                        "enabled" => {
                            cfg.enabled = v.as_bool().ok_or_else(|| {
                                WritingError::bad_arg("rules", "`enabled` must be a boolean")
                            })?;
                        }
                        "severity" => {
                            cfg.severity =
                                v.as_str().and_then(DiagSeverity::parse).ok_or_else(|| {
                                    WritingError::bad_arg(
                                        "rules",
                                        "`severity` must be error, warning or info",
                                    )
                                })?;
                        }
                        "max_words" => {
                            cfg.max_words = v
                                .as_u64()
                                .and_then(|n| usize::try_from(n).ok())
                                .filter(|&n| n > 0)
                                .ok_or_else(|| {
                                    WritingError::bad_arg(
                                        "rules",
                                        "`max_words` must be a positive integer",
                                    )
                                })?;
                        }
                        "extra_words" => cfg.extra_words = string_list(v, "extra_words")?,
                        "ignore_words" => cfg.ignore_words = string_list(v, "ignore_words")?,
                        other => {
                            return Err(WritingError::bad_arg(
                                "rules",
                                format!("unknown setting `{other}` for rule `{}`", rule.id()),
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(WritingError::bad_arg(
                    "rules",
                    format!("rule `{}` expects a boolean or an object", rule.id()),
                ));
            }
        }
        Ok(())
    }

    fn get(&self, rule: Rule) -> Option<&RuleConfig> {
        self.rules.get(&rule).filter(|c| c.enabled)
    }
}

fn rule_from(id: &str) -> Result<Rule, WritingError> {
    Rule::parse(id).ok_or_else(|| {
        let known: Vec<_> = Rule::ALL.iter().map(|r| r.id()).collect();
        WritingError::bad_arg(
            "rules",
            format!("unknown rule `{id}`; known rules: {}", known.join(", ")),
        )
    })
}

fn string_list(value: &Value, field: &'static str) -> Result<Vec<String>, WritingError> {
    let items = value.as_array().ok_or_else(|| {
        WritingError::bad_arg("rules", format!("`{field}` must be an array of strings"))
    })?;
    items
        .iter()
        .map(|v| {
            v.as_str().map(str::to_lowercase).ok_or_else(|| {
                WritingError::bad_arg("rules", format!("`{field}` must be an array of strings"))
            })
        })
        .collect()
}

/// One finding before conversion to a [`Diagnostic`].
struct Finding {
    rule: Rule,
    start: usize,
    end: usize,
    message: String,
    hints: Vec<String>,
}

/// Lint masked prose. `file` is copied into each diagnostic.
pub fn lint(masked: &str, config: &LintConfig, file: Option<&str>) -> Vec<Diagnostic> {
    let all_words = words(masked);
    let lower: Vec<String> = all_words
        .iter()
        .map(|w| w.text(masked).to_lowercase())
        .collect();
    let mut found = Vec::new();
    let ctx = Ctx {
        text: masked,
        words: &all_words,
        lower: &lower,
    };
    if let Some(c) = config.get(Rule::RepeatedWord) {
        repeated_words(&ctx, c, &mut found);
    }
    if let Some(c) = config.get(Rule::LongSentence) {
        long_sentences(&ctx, c, &mut found);
    }
    if let Some(c) = config.get(Rule::PassiveVoice) {
        passive_voice(&ctx, c, &mut found);
    }
    if let Some(c) = config.get(Rule::WeaselWords) {
        weasel_words(&ctx, c, &mut found);
    }
    if config.get(Rule::DoubleSpace).is_some() {
        double_spaces(masked, &mut found);
    }
    if config.get(Rule::Hyphenation).is_some() {
        hyphenation(&ctx, &mut found);
    }
    found.sort_by_key(|f| (f.start, f.end, f.rule));

    let index = TextIndex::new(masked);
    found
        .into_iter()
        .take(MAX_DIAGNOSTICS)
        .map(|f| {
            let severity = config
                .rules
                .get(&f.rule)
                .map_or(f.rule.default_severity(), |c| c.severity);
            Diagnostic::new(
                &index,
                file,
                f.rule.id(),
                severity,
                index.char_range(f.start..f.end),
                f.message,
                f.hints,
            )
        })
        .collect()
}

struct Ctx<'a> {
    text: &'a str,
    words: &'a [Word],
    lower: &'a [String],
}

impl Ctx<'_> {
    /// Text between word `i` and word `i + 1`.
    fn gap(&self, i: usize) -> &str {
        &self.text[self.words[i].end..self.words[i + 1].start]
    }

    /// True when words `i` and `i + 1` are separated by plain whitespace on
    /// at most one line break.
    fn adjacent(&self, i: usize) -> bool {
        let gap = self.gap(i);
        !gap.is_empty() && gap.chars().all(char::is_whitespace) && gap.matches('\n').count() <= 1
    }
}

// ---------------------------------------------------------------------------

/// Doubles that are grammatical.
const ALLOWED_DOUBLES: &[&str] = &["had", "that"];

fn repeated_words(ctx: &Ctx<'_>, cfg: &RuleConfig, out: &mut Vec<Finding>) {
    for i in 0..ctx.words.len().saturating_sub(1) {
        let (a, b) = (&ctx.lower[i], &ctx.lower[i + 1]);
        if a != b
            || !ctx.adjacent(i)
            || a.chars().any(|c| c.is_numeric())
            || ALLOWED_DOUBLES.contains(&a.as_str())
            || cfg.ignore_words.contains(a)
        {
            continue;
        }
        out.push(Finding {
            rule: Rule::RepeatedWord,
            start: ctx.words[i].start,
            end: ctx.words[i + 1].end,
            message: format!("Repeated word `{}`", ctx.words[i].text(ctx.text)),
            hints: vec!["Remove the duplicated word.".to_owned()],
        });
    }
}

fn long_sentences(ctx: &Ctx<'_>, cfg: &RuleConfig, out: &mut Vec<Finding>) {
    for s in sentences(ctx.text, ctx.words) {
        if s.words > cfg.max_words {
            out.push(Finding {
                rule: Rule::LongSentence,
                start: s.start,
                end: s.end,
                message: format!("Sentence has {} words (limit {})", s.words, cfg.max_words),
                hints: vec!["Consider splitting it into shorter sentences.".to_owned()],
            });
        }
    }
}

const AUXILIARIES: &[&str] = &["is", "are", "was", "were", "be", "been", "being", "am"];

const FILLER_ADVERBS: &[&str] = &[
    "not",
    "also",
    "often",
    "usually",
    "always",
    "never",
    "still",
    "then",
    "just",
    "now",
    "typically",
    "generally",
    "sometimes",
];

const IRREGULAR_PARTICIPLES: &[&str] = &[
    "built",
    "made",
    "given",
    "taken",
    "shown",
    "seen",
    "known",
    "found",
    "done",
    "written",
    "chosen",
    "driven",
    "broken",
    "spoken",
    "held",
    "kept",
    "left",
    "lost",
    "paid",
    "sent",
    "begun",
    "drawn",
    "grown",
    "thrown",
    "worn",
    "torn",
    "born",
    "eaten",
    "fallen",
    "forgotten",
    "hidden",
    "risen",
    "taught",
    "understood",
    "withdrawn",
    "won",
    "led",
    "shot",
    "bought",
    "caught",
    "brought",
    "told",
    "sold",
    "proven",
    "gone",
    "met",
    "overcome",
    "undone",
];

/// Words ending in `-ed` that are not participles, and participles that are
/// almost always adjectives.
const NOT_PASSIVE: &[&str] = &[
    "need",
    "speed",
    "seed",
    "feed",
    "bleed",
    "breed",
    "indeed",
    "embed",
    "proceed",
    "exceed",
    "succeed",
    "hundred",
    "shed",
    "sled",
    "weed",
    "deed",
    "creed",
    "greed",
    "steed",
    "reed",
    "naked",
    "sacred",
    "wicked",
    "tired",
    "interested",
    "excited",
    "bored",
    "surprised",
    "pleased",
    "concerned",
    "supposed",
    "married",
];

fn participle(word: &str) -> bool {
    !NOT_PASSIVE.contains(&word)
        && (IRREGULAR_PARTICIPLES.contains(&word) || (word.len() >= 4 && word.ends_with("ed")))
}

fn passive_voice(ctx: &Ctx<'_>, cfg: &RuleConfig, out: &mut Vec<Finding>) {
    let n = ctx.words.len();
    for i in 0..n {
        if !AUXILIARIES.contains(&ctx.lower[i].as_str()) {
            continue;
        }
        let mut j = i;
        let mut skipped = 0;
        loop {
            if j + 1 >= n || !ctx.adjacent(j) {
                break;
            }
            j += 1;
            let w = ctx.lower[j].as_str();
            let adverb = FILLER_ADVERBS.contains(&w) || (w.len() > 4 && w.ends_with("ly"));
            if adverb && skipped < 2 {
                skipped += 1;
                continue;
            }
            if participle(w) && !cfg.ignore_words.iter().any(|x| x == w) {
                let by = j + 1 < n && ctx.adjacent(j) && ctx.lower[j + 1] == "by";
                out.push(Finding {
                    rule: Rule::PassiveVoice,
                    start: ctx.words[i].start,
                    end: ctx.words[j].end,
                    message: format!(
                        "Possible passive voice: `{} {}`",
                        ctx.words[i].text(ctx.text),
                        ctx.words[j].text(ctx.text)
                    ),
                    hints: vec![if by {
                        "Name the actor as the subject and use an active verb.".to_owned()
                    } else {
                        "Consider rewriting in the active voice.".to_owned()
                    }],
                });
            }
            break;
        }
    }
}

const WEASEL_WORDS: &[&str] = &[
    "very",
    "quite",
    "several",
    "various",
    "many",
    "few",
    "fairly",
    "rather",
    "somewhat",
    "clearly",
    "obviously",
    "simply",
    "basically",
    "really",
    "extremely",
    "relatively",
    "remarkably",
    "arguably",
    "virtually",
    "mostly",
    "probably",
    "perhaps",
    "maybe",
    "essentially",
    "actually",
    "literally",
    "highly",
];

const WEASEL_PHRASES: &[&[&str]] = &[
    &["a", "number", "of"],
    &["a", "lot", "of"],
    &["it", "is", "well", "known"],
    &["as", "is", "well", "known"],
    &["it", "is", "obvious"],
    &["it", "is", "clear", "that"],
    &["needless", "to", "say"],
    &["it", "goes", "without", "saying"],
];

fn weasel_words(ctx: &Ctx<'_>, cfg: &RuleConfig, out: &mut Vec<Finding>) {
    let n = ctx.words.len();
    let mut i = 0;
    'words: while i < n {
        for phrase in WEASEL_PHRASES {
            let len = phrase.len();
            if i + len <= n
                && (0..len).all(|k| ctx.lower[i + k] == phrase[k])
                && (0..len - 1).all(|k| ctx.adjacent(i + k))
            {
                out.push(weasel(ctx, i, i + len - 1));
                i += len;
                continue 'words;
            }
        }
        let w = ctx.lower[i].as_str();
        let flagged = (WEASEL_WORDS.contains(&w) || cfg.extra_words.iter().any(|x| x == w))
            && !cfg.ignore_words.iter().any(|x| x == w);
        if flagged {
            out.push(weasel(ctx, i, i));
        }
        i += 1;
    }
}

fn weasel(ctx: &Ctx<'_>, first: usize, last: usize) -> Finding {
    let start = ctx.words[first].start;
    let end = ctx.words[last].end;
    Finding {
        rule: Rule::WeaselWords,
        start,
        end,
        message: format!("Vague wording: `{}`", &ctx.text[start..end]),
        hints: vec!["Be specific, or cut it.".to_owned()],
    }
}

fn double_spaces(text: &str, out: &mut Vec<Finding>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b' ' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i - start < 2 {
            continue;
        }
        let before = text[..start].chars().next_back();
        let after = text[i..].chars().next();
        let inner = |c: Option<char>| c.is_some_and(|c| !c.is_whitespace() && c != OPAQUE);
        if inner(before) && inner(after) {
            out.push(Finding {
                rule: Rule::DoubleSpace,
                start,
                end: i,
                message: "Multiple spaces in a row".to_owned(),
                hints: vec!["Use a single space.".to_owned()],
            });
        }
    }
}

#[derive(Default)]
struct Forms {
    hyphen: Vec<(usize, usize)>,
    spaced: Vec<(usize, usize)>,
    closed: Vec<(usize, usize)>,
    parts: Vec<String>,
}

fn split_hyphens(word: &str) -> Vec<&str> {
    word.split(['-', '\u{2010}', '\u{2011}']).collect()
}

fn hyphenation(ctx: &Ctx<'_>, out: &mut Vec<Finding>) {
    let mut groups: HashMap<String, Forms> = HashMap::new();
    for (i, w) in ctx.lower.iter().enumerate() {
        if !w.contains(['-', '\u{2010}', '\u{2011}']) {
            continue;
        }
        let parts = split_hyphens(w);
        if parts.len() < 2
            || parts
                .iter()
                .any(|p| p.is_empty() || p.chars().any(|c| c.is_numeric()))
        {
            continue;
        }
        let key = parts.concat();
        let g = groups.entry(key).or_default();
        if g.parts.is_empty() {
            g.parts = parts.iter().map(|p| (*p).to_owned()).collect();
        }
        g.hyphen.push((ctx.words[i].start, ctx.words[i].end));
    }
    if groups.is_empty() {
        return;
    }
    let mut by_first: HashMap<&str, Vec<&str>> = HashMap::new();
    for (key, g) in &groups {
        by_first.entry(g.parts[0].as_str()).or_default().push(key);
    }
    let mut spaced_hits = Vec::new();
    let mut closed_hits = Vec::new();
    for (i, w) in ctx.lower.iter().enumerate() {
        if groups.contains_key(w.as_str()) {
            closed_hits.push((w.clone(), ctx.words[i].start, ctx.words[i].end));
        }
        if let Some(keys) = by_first.get(w.as_str()) {
            for key in keys {
                let parts = &groups[*key].parts;
                let n = parts.len();
                if i + n <= ctx.lower.len()
                    && (0..n).all(|k| ctx.lower[i + k] == parts[k])
                    && (0..n - 1).all(|k| ctx.adjacent(i + k))
                {
                    spaced_hits.push((
                        (*key).to_owned(),
                        ctx.words[i].start,
                        ctx.words[i + n - 1].end,
                    ));
                }
            }
        }
    }
    for (key, s, e) in spaced_hits {
        if let Some(g) = groups.get_mut(&key) {
            g.spaced.push((s, e));
        }
    }
    for (key, s, e) in closed_hits {
        if let Some(g) = groups.get_mut(&key) {
            g.closed.push((s, e));
        }
    }

    let mut keys: Vec<&String> = groups.keys().collect();
    keys.sort();
    for key in keys {
        let g = &groups[key];
        type Occurrences<'a> = &'a [(usize, usize)];
        let forms: [(&str, Occurrences<'_>, String); 3] = [
            ("hyphenated", &g.hyphen, g.parts.join("-")),
            ("spaced", &g.spaced, g.parts.join(" ")),
            ("closed", &g.closed, g.parts.concat()),
        ];
        let present: Vec<_> = forms.iter().filter(|(_, v, _)| !v.is_empty()).collect();
        if present.len() < 2 {
            continue;
        }
        // Majority wins; a tie goes to the form that appears first.
        let Some(preferred) = present.iter().max_by_key(|(_, v, _)| {
            (
                v.len(),
                std::cmp::Reverse(v.iter().map(|r| r.0).min().unwrap_or(usize::MAX)),
            )
        }) else {
            continue;
        };
        for (name, occurrences, shown) in &present {
            if name == &preferred.0 {
                continue;
            }
            for &(start, end) in occurrences.iter() {
                out.push(Finding {
                    rule: Rule::Hyphenation,
                    start,
                    end,
                    message: format!(
                        "Inconsistent hyphenation: `{shown}` is also written `{}`",
                        preferred.2
                    ),
                    hints: vec![format!("Use `{}` consistently.", preferred.2)],
                });
            }
        }
    }
}

/// Convenience for tests and callers that only need ranges.
pub fn ranges(diags: &[Diagnostic]) -> Vec<CharRange> {
    diags.iter().filter_map(|d| d.range).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::prose;
    use crate::syntax::Syntax;
    use serde_json::json;

    fn run(text: &str, rules: Option<Value>) -> Vec<Diagnostic> {
        let config = LintConfig::from_json(rules.as_ref()).expect("config");
        lint(&prose(text, Syntax::Plain, &[]), &config, None)
    }

    fn codes(d: &[Diagnostic]) -> Vec<&str> {
        d.iter().filter_map(|d| d.code.as_deref()).collect()
    }

    #[test]
    fn repeated_words() {
        let d = run(
            "This is is a test. And the\nthe end. He had had enough. 1 1",
            None,
        );
        assert_eq!(codes(&d), ["repeated-word", "repeated-word"]);
        assert_eq!(d[0].range, Some(CharRange { start: 5, end: 10 }));
        assert_eq!(d[0].severity, DiagSeverity::Warning);
    }

    #[test]
    fn repeats_across_masked_math_are_not_repeats() {
        let text = "the $x$ the and the #f(1) the";
        let masked = prose(text, Syntax::Typst, &[]);
        assert!(lint(&masked, &LintConfig::default(), None).is_empty());
    }

    #[test]
    fn long_sentences_respect_threshold() {
        let sentence = (0..12).map(|i| format!("w{i} ")).collect::<String>() + "end.";
        let d = run(&sentence, Some(json!({"long-sentence": {"max_words": 10}})));
        assert_eq!(codes(&d), ["long-sentence"]);
        assert!(run(&sentence, None).is_empty());
    }

    #[test]
    fn passive_voice() {
        let d = run(
            "The sample was heated quickly. Results were not shown. It was done by us. We need speed. The door is red.",
            None,
        );
        assert_eq!(
            codes(&d),
            ["passive-voice", "passive-voice", "passive-voice"]
        );
        assert!(d[2].hints[0].contains("actor"));
    }

    #[test]
    fn weasel_words_and_phrases() {
        let d = run(
            "It is very clear. A number of results, arguably, matter.",
            None,
        );
        let msgs: Vec<_> = d.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            msgs,
            [
                "Vague wording: `very`",
                "Vague wording: `A number of`",
                "Vague wording: `arguably`"
            ]
        );
        let extra = run(
            "It is gnarly.",
            Some(json!({"weasel-words": {"extra_words": ["Gnarly"]}})),
        );
        assert_eq!(extra.len(), 1);
    }

    #[test]
    fn double_spaces_only_inside_prose() {
        let d = run("One  two.   Three\n  indented. trailing  ", None);
        assert_eq!(codes(&d), ["double-space", "double-space"]);
        let masked = prose("a  $x$  b", Syntax::Typst, &[]);
        assert!(lint(&masked, &LintConfig::default(), None).is_empty());
    }

    #[test]
    fn hyphenation_majority_wins() {
        let d = run(
            "A well-known fact. A well-known idea. A well known thing. Also wellknown.",
            None,
        );
        assert_eq!(codes(&d), ["hyphenation", "hyphenation"]);
        assert!(d[0].message.contains("well known"));
        assert!(d[1].hints[0].contains("well-known"));
    }

    #[test]
    fn rule_selection_and_severity() {
        let text = "The the very sample was heated.";
        assert_eq!(
            codes(&run(text, Some(json!(["repeated-word"])))),
            ["repeated-word"]
        );
        let d = run(
            text,
            Some(json!({"repeated-word": {"severity": "error"}, "passive-voice": false})),
        );
        assert_eq!(d[0].severity, DiagSeverity::Error);
        assert!(!codes(&d).contains(&"passive-voice"));
        assert!(LintConfig::from_json(Some(&json!({"nope": true}))).is_err());
        assert!(LintConfig::from_json(Some(&json!({"long-sentence": {"max_words": 0}}))).is_err());
        assert!(LintConfig::from_json(Some(&json!(3))).is_err());
    }
}
