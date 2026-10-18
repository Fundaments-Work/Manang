//! The curated `@preview` table: names that belong to a known package.
//!
//! Every package, version and exported name below was checked against the
//! Typst Universe index (`packages.typst.org/preview/index.json`) and the
//! package sources on 2026-10-18. Versions are the latest at that date; bump
//! them together with [`VERIFIED_ON`]. A name that two packages export
//! is listed under both, and both are offered.

/// How a symbol is brought into scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Import {
    /// `#import "@preview/pkg:ver": name`
    Named,
    /// `#import "@preview/pkg:ver"` binds the package name itself.
    Module,
    /// `#import "@preview/pkg:ver" as alias`
    Alias(&'static str),
    /// A touying theme: `#import "@preview/touying:ver": *` and
    /// `#import themes.<module>: *`.
    Theme(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct Package {
    pub name: &'static str,
    pub version: &'static str,
    pub symbols: &'static [(&'static str, Import)],
}

/// Date the table was verified, `YYYY-MM-DD`.
pub const VERIFIED_ON: &str = "2026-10-18";

use Import::{Alias, Module, Named, Theme};

pub const PACKAGES: &[Package] = &[
    Package {
        name: "cetz",
        version: "0.5.2",
        symbols: &[("cetz", Module), ("canvas", Named), ("draw", Named)],
    },
    Package {
        name: "cetz-plot",
        version: "0.1.4",
        symbols: &[("plot", Named), ("chart", Named)],
    },
    Package {
        name: "fletcher",
        version: "0.5.8",
        symbols: &[("diagram", Named), ("node", Named), ("edge", Named)],
    },
    Package {
        name: "touying",
        version: "0.8.0",
        symbols: &[
            ("pause", Named),
            ("meanwhile", Named),
            ("uncover", Named),
            ("only", Named),
            ("config-page", Named),
            ("config-info", Named),
            ("config-colors", Named),
            ("touying-slide-wrapper", Named),
            ("simple-theme", Theme("simple")),
            ("metropolis-theme", Theme("metropolis")),
            ("university-theme", Theme("university")),
            ("dewdrop-theme", Theme("dewdrop")),
            ("stargazer-theme", Theme("stargazer")),
            ("aqua-theme", Theme("aqua")),
            ("default-theme", Theme("default")),
        ],
    },
    Package {
        name: "tablex",
        version: "0.0.9",
        symbols: &[
            ("tablex", Named),
            ("gridx", Named),
            ("cellx", Named),
            ("rowspanx", Named),
            ("colspanx", Named),
            ("hlinex", Named),
            ("vlinex", Named),
        ],
    },
    Package {
        name: "codly",
        version: "2.0.0",
        symbols: &[("codly", Named)],
    },
    Package {
        name: "ctheorems",
        version: "2.0.0",
        symbols: &[
            ("thm-rules", Named),
            ("thm", Named),
            ("proof", Named),
            ("qedhere", Named),
        ],
    },
    Package {
        name: "showybox",
        version: "2.0.4",
        symbols: &[("showybox", Named)],
    },
    Package {
        name: "physica",
        version: "0.9.8",
        symbols: &[
            ("dd", Named),
            ("pdv", Named),
            ("ket", Named),
            ("bra", Named),
            ("braket", Named),
            ("grad", Named),
            ("curl", Named),
            ("laplacian", Named),
            ("expval", Named),
            ("tensor", Named),
            ("vecrow", Named),
        ],
    },
    Package {
        name: "mitex",
        version: "0.2.7",
        symbols: &[("mitex", Named), ("mi", Named)],
    },
    Package {
        name: "unify",
        version: "0.8.1",
        symbols: &[
            ("num", Named),
            ("qty", Named),
            ("unit", Named),
            ("numrange", Named),
            ("qtyrange", Named),
        ],
    },
    Package {
        name: "lilaq",
        version: "0.6.0",
        symbols: &[("lq", Alias("lq"))],
    },
    Package {
        name: "pinit",
        version: "0.2.2",
        symbols: &[
            ("pinit", Named),
            ("pinit-highlight", Named),
            ("pinit-arrow", Named),
            ("pinit-point-from", Named),
        ],
    },
    Package {
        name: "zebraw",
        version: "0.6.3",
        symbols: &[("zebraw", Named), ("zebraw-init", Named)],
    },
    Package {
        name: "tablem",
        version: "0.3.0",
        symbols: &[("tablem", Named), ("three-line-table", Named)],
    },
    Package {
        name: "algorithmic",
        version: "1.0.7",
        symbols: &[("algorithm-figure", Named), ("style-algorithm", Named)],
    },
];

/// Every `(package, how)` that provides `name`.
pub fn providers(name: &str) -> Vec<(&'static Package, Import)> {
    PACKAGES
        .iter()
        .filter_map(|p| {
            p.symbols
                .iter()
                .find(|(symbol, _)| *symbol == name)
                .map(|(_, how)| (p, *how))
        })
        .collect()
}

/// Every symbol name in the table, for misspelling checks.
pub fn all_symbols() -> impl Iterator<Item = &'static str> {
    PACKAGES
        .iter()
        .flat_map(|p| p.symbols.iter().map(|(name, _)| *name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_per_package_and_versions_look_like_semver() {
        for p in PACKAGES {
            let parts: Vec<&str> = p.version.split('.').collect();
            assert_eq!(parts.len(), 3, "{}", p.name);
            assert!(parts.iter().all(|n| n.parse::<u32>().is_ok()), "{}", p.name);
            let mut names: Vec<&str> = p.symbols.iter().map(|(n, _)| *n).collect();
            names.sort_unstable();
            let before = names.len();
            names.dedup();
            assert_eq!(before, names.len(), "{}", p.name);
        }
    }

    #[test]
    fn lookup() {
        assert_eq!(providers("canvas").len(), 1);
        assert_eq!(providers("canvas")[0].0.name, "cetz");
        assert!(providers("nonesuch").is_empty());
    }
}
