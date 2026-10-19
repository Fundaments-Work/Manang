//! Searching, filtering and ordering the index, and comparing it with what is
//! installed. Pure functions over a parsed [`Index`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::cache::Stats;
use crate::index::{Index, PluginEntry, is_newer};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    /// A to Z.
    #[default]
    Name,
    /// Most recently published latest version first.
    Newest,
    /// Most downloaded first.
    Downloads,
}

impl Sort {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "name" => Some(Self::Name),
            "newest" => Some(Self::Newest),
            "downloads" => Some(Self::Downloads),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Every word must appear in the name, id, author, description or a tag
    /// (case-insensitive).
    pub text: String,
    /// Only plugins carrying this tag.
    pub tag: Option<String>,
    pub sort: Sort,
}

fn matches(p: &PluginEntry, words: &[String], tag: Option<&str>) -> bool {
    if let Some(tag) = tag
        && !p.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    {
        return false;
    }
    if words.is_empty() {
        return true;
    }
    let haystack = [
        p.name.as_str(),
        p.id.as_str(),
        p.author.as_str(),
        p.description.as_str(),
    ]
    .iter()
    .map(|s| s.to_lowercase())
    .chain(p.tags.iter().map(|t| t.to_lowercase()))
    .collect::<Vec<_>>()
    .join("\n");
    words.iter().all(|w| haystack.contains(w.as_str()))
}

fn published(p: &PluginEntry) -> &str {
    p.latest_version().map_or("", |v| v.published.as_str())
}

/// The plugins matching `query`, in the requested order (ties by name).
pub fn search<'a>(index: &'a Index, stats: &Stats, query: &Query) -> Vec<&'a PluginEntry> {
    let words: Vec<String> = query
        .text
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let mut found: Vec<&PluginEntry> = index
        .plugins
        .iter()
        .filter(|p| matches(p, &words, query.tag.as_deref()))
        .collect();
    let by_name = |a: &&PluginEntry, b: &&PluginEntry| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    };
    match query.sort {
        Sort::Name => found.sort_by(by_name),
        Sort::Newest => {
            found.sort_by(|a, b| published(b).cmp(published(a)).then_with(|| by_name(a, b)))
        }
        Sort::Downloads => found.sort_by(|a, b| {
            let count = |p: &PluginEntry| stats.downloads.get(&p.id).copied().unwrap_or(0);
            count(b).cmp(&count(a)).then_with(|| by_name(a, b))
        }),
    }
    found
}

/// Every tag with the number of plugins carrying it, most common first.
pub fn tags(index: &Index) -> Vec<(String, usize)> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for p in &index.plugins {
        for t in &p.tags {
            *counts.entry(t.to_lowercase()).or_default() += 1;
        }
    }
    let mut list: Vec<_> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    list
}

/// A newer version of an installed plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Update {
    pub id: String,
    pub installed: String,
    pub latest: String,
}

/// For each `(id, installed version)` the index knows, whether `latest` is
/// newer. Plugins the index does not list are not reported.
pub fn updates(index: &Index, installed: &[(String, String)]) -> Vec<Update> {
    installed
        .iter()
        .filter_map(|(id, version)| {
            let p = index.plugin(id)?;
            is_newer(&p.latest, version).then(|| Update {
                id: id.clone(),
                installed: version.clone(),
                latest: p.latest.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{Permissions, VersionEntry};

    fn plugin(id: &str, name: &str, tags: &[&str], published: &str, latest: &str) -> PluginEntry {
        PluginEntry {
            id: id.to_owned(),
            name: name.to_owned(),
            author: "Someone".to_owned(),
            description: format!("{name} does a thing"),
            repo: "https://example.org".to_owned(),
            homepage: None,
            license: "MIT".to_owned(),
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
            tier: "wasm".to_owned(),
            readme: None,
            latest: latest.to_owned(),
            versions: vec![VersionEntry {
                version: latest.to_owned(),
                published: published.to_owned(),
                min_app: None,
                download: "x".to_owned(),
                source: None,
                size: 1,
                sha256: "0".repeat(64),
                permissions: Permissions::default(),
                changelog: String::new(),
            }],
        }
    }

    fn index() -> Index {
        Index {
            schema: 1,
            generated: "t".to_owned(),
            plugins: vec![
                plugin("zeta", "Zeta", &["writing"], "2026-03-01", "1.0.0"),
                plugin(
                    "alpha",
                    "Alpha",
                    &["writing", "math"],
                    "2026-01-01",
                    "0.2.0",
                ),
                plugin("mid", "Mid", &["math"], "2026-06-01", "0.10.0"),
            ],
        }
    }

    fn ids(found: &[&PluginEntry]) -> Vec<String> {
        found.iter().map(|p| p.id.clone()).collect()
    }

    #[test]
    fn sorts_by_name_newest_and_downloads() {
        let i = index();
        let stats = Stats {
            downloads: [("zeta".to_owned(), 5), ("mid".to_owned(), 90)].into(),
        };
        let q = |sort| Query {
            sort,
            ..Query::default()
        };
        assert_eq!(
            ids(&search(&i, &stats, &q(Sort::Name))),
            ["alpha", "mid", "zeta"]
        );
        assert_eq!(
            ids(&search(&i, &stats, &q(Sort::Newest))),
            ["mid", "zeta", "alpha"]
        );
        assert_eq!(
            ids(&search(&i, &stats, &q(Sort::Downloads))),
            ["mid", "zeta", "alpha"]
        );
    }

    #[test]
    fn text_matches_every_word_across_fields_and_tag_filters() {
        let i = index();
        let none = Stats::default();
        let text = |t: &str| Query {
            text: t.to_owned(),
            ..Query::default()
        };
        assert_eq!(ids(&search(&i, &none, &text("ALPHA math"))), ["alpha"]);
        assert_eq!(ids(&search(&i, &none, &text("someone"))).len(), 3);
        assert!(search(&i, &none, &text("nothing like this")).is_empty());
        let tagged = Query {
            tag: Some("Math".to_owned()),
            ..Query::default()
        };
        assert_eq!(ids(&search(&i, &none, &tagged)), ["alpha", "mid"]);
        assert_eq!(tags(&i)[0].1, 2);
    }

    #[test]
    fn detects_updates_with_semver_and_ignores_unlisted_plugins() {
        let i = index();
        let installed = [
            ("mid".to_owned(), "0.9.0".to_owned()),
            ("zeta".to_owned(), "1.0.0".to_owned()),
            ("alpha".to_owned(), "0.3.0".to_owned()),
            ("elsewhere".to_owned(), "0.1.0".to_owned()),
        ];
        let found = updates(&i, &installed);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "mid");
        assert_eq!(found[0].latest, "0.10.0");
    }
}
