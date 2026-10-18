//! `git.identity-get` / `git.identity-set` against an isolated, empty global
//! config (own process; a single sequential test because the global scope is
//! process-wide).

mod common;

use common::{Repo, git, harness};
use serde_json::json;

#[test]
fn identity_scopes_and_first_commit_flow() {
    let h = harness();
    let r = Repo::plain();
    git(&r.dir, &["init", "-q", "-b", "main"]);

    // Nothing anywhere.
    let none = h.ok(&r, "git.identity-get", json!({}));
    assert_eq!(none["source"], "none");
    assert!(none["name"].is_null() && none["email"].is_null());

    // The first save asks for an identity instead of guessing one.
    r.write("a.txt", "x\n");
    assert_eq!(
        h.code(&r, "git.save-version", json!({"name": "v"})),
        "missing-identity"
    );

    // Validation.
    for (name, email) in [
        ("", "a@b.org"),
        ("Ada\nL", "a@b.org"),
        ("Ada", "not-an-email"),
        ("Ada", "a@b.org\n"),
        ("Ada", "a b@c.org"),
    ] {
        assert_eq!(
            h.code(
                &r,
                "git.identity-set",
                json!({"name": name, "email": email})
            ),
            "bad-arguments",
            "{name:?} {email:?}"
        );
    }

    // Repo scope by default.
    let ok = h.ok(
        &r,
        "git.identity-set",
        json!({"name": "Ada Lovelace", "email": "ada@example.org"}),
    );
    assert_eq!(ok["ok"], true);
    let got = h.ok(&r, "git.identity-get", json!({}));
    assert_eq!(got["source"], "repo");
    assert_eq!(got["name"], "Ada Lovelace");
    assert_eq!(got["email"], "ada@example.org");
    assert_eq!(
        git(&r.dir, &["config", "--local", "user.name"]),
        "Ada Lovelace"
    );
    h.ok(&r, "git.save-version", json!({"name": "v1"}));
    assert_eq!(
        git(&r.dir, &["log", "-1", "--format=%an <%ae>"]),
        "Ada Lovelace <ada@example.org>"
    );

    // Global scope lands in the (temporary) global config and is inherited.
    let other = Repo::plain();
    git(&other.dir, &["init", "-q", "-b", "main"]);
    h.ok(
        &other,
        "git.identity-set",
        json!({"name": "Grace", "email": "grace@example.org", "global": true}),
    );
    let got = h.ok(&other, "git.identity-get", json!({}));
    assert_eq!(got["source"], "global");
    assert_eq!(got["name"], "Grace");
    let home_cfg = std::env::var("GIT_CONFIG_GLOBAL").expect("isolated");
    let text = std::fs::read_to_string(home_cfg).expect("global config");
    assert!(text.contains("grace@example.org"), "{text}");
    // The first repository keeps its own identity.
    assert_eq!(
        h.ok(&r, "git.identity-get", json!({}))["name"],
        "Ada Lovelace"
    );
    other.write("a.txt", "x\n");
    h.ok(&other, "git.save-version", json!({"name": "v"}));
}
