//! Set arithmetic on a manifest's [`Permissions`], used to compare what a
//! plugin asks for with what the user approved.

use std::collections::BTreeSet;

use manang_kernel::manifest::Permissions;

fn key(s: &str, lowercase: bool) -> String {
    if lowercase {
        s.to_ascii_lowercase()
    } else {
        s.to_owned()
    }
}

fn set(list: &[String], lowercase: bool) -> BTreeSet<String> {
    list.iter().map(|s| key(s, lowercase)).collect()
}

fn subset(a: &[String], b: &[String], lowercase: bool) -> bool {
    set(a, lowercase).is_subset(&set(b, lowercase))
}

/// Entries of `a` whose membership in `b` equals `keep_if_in_b`, de-duplicated.
fn filter(a: &[String], b: &[String], lowercase: bool, keep_if_in_b: bool) -> Vec<String> {
    let b = set(b, lowercase);
    let mut seen = BTreeSet::new();
    a.iter()
        .filter(|s| {
            let k = key(s, lowercase);
            b.contains(&k) == keep_if_in_b && seen.insert(k)
        })
        .cloned()
        .collect()
}

/// Whether every permission in `a` is also in `b`.
pub fn is_subset(a: &Permissions, b: &Permissions) -> bool {
    subset(&a.filesystem, &b.filesystem, false)
        && subset(&a.network, &b.network, true)
        && subset(&a.buffer, &b.buffer, false)
}

/// What `a` asks for beyond `b`.
pub fn difference(a: &Permissions, b: &Permissions) -> Permissions {
    Permissions {
        filesystem: filter(&a.filesystem, &b.filesystem, false, false),
        network: filter(&a.network, &b.network, true, false),
        buffer: filter(&a.buffer, &b.buffer, false, false),
    }
}

/// What both grant.
pub fn intersection(a: &Permissions, b: &Permissions) -> Permissions {
    Permissions {
        filesystem: filter(&a.filesystem, &b.filesystem, false, true),
        network: filter(&a.network, &b.network, true, true),
        buffer: filter(&a.buffer, &b.buffer, false, true),
    }
}

/// True when nothing is requested.
pub fn is_empty(p: &Permissions) -> bool {
    p.filesystem.is_empty() && p.network.is_empty() && p.buffer.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(fs: &[&str], net: &[&str], buf: &[&str]) -> Permissions {
        let v = |x: &[&str]| x.iter().map(|s| (*s).to_owned()).collect();
        Permissions {
            filesystem: v(fs),
            network: v(net),
            buffer: v(buf),
        }
    }

    #[test]
    fn subset_difference_and_intersection_agree() {
        let granted = p(&["read:templates"], &["Example.org"], &[]);
        let asked = p(
            &["read:templates", "write:out"],
            &["example.org"],
            &["read:active"],
        );
        assert!(is_subset(&granted, &asked));
        assert!(!is_subset(&asked, &granted));
        let extra = difference(&asked, &granted);
        assert_eq!(extra, p(&["write:out"], &[], &["read:active"]));
        assert_eq!(
            intersection(&asked, &granted),
            p(&["read:templates"], &["example.org"], &[])
        );
        assert!(is_empty(&difference(&granted, &asked)));
    }
}
