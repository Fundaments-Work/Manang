//! The invite string: where the relay is, which file, and the key.
//!
//! ```text
//! manang-live://join?relay=wss%3A%2F%2Frelay.example.org&file=chapters%2Fintro.typ#<secret>
//! ```
//!
//! Everything after `#` is the room secret plus the host's start-text
//! fingerprint (see [`manang_live::envelope::Invite`]). It is parsed here and
//! never leaves the process: the relay is only ever given the room id derived
//! from it, and the relay address and file name are not secrets.

use url::Url;

use crate::error::LiveHostError;

const SCHEME: &str = "manang-live";

/// A parsed invite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InviteLink {
    /// Relay origin, `ws://host:port` or `wss://host`.
    pub relay: String,
    /// The host's file, relative to the project (a hint for opening it).
    pub file: Option<String>,
    /// The secret part, as [`manang_live::envelope::Invite::from_fragment`] reads it.
    pub fragment: String,
}

/// Reduce a user-entered relay address to an origin `ws[s]://host[:port]`.
///
/// Accepts `http`/`https` as aliases, refuses credentials and anything that is
/// not a network address. Paths, queries and fragments are dropped.
pub fn normalize_relay(input: &str) -> Result<String, LiveHostError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(LiveHostError::NoRelay);
    }
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("wss://{trimmed}")
    };
    let url = Url::parse(&with_scheme).map_err(|e| LiveHostError::BadRelay(e.to_string()))?;
    let scheme = match url.scheme() {
        "ws" | "http" => "ws",
        "wss" | "https" => "wss",
        other => {
            return Err(LiveHostError::BadRelay(format!(
                "use ws:// or wss://, not {other}://"
            )));
        }
    };
    if !url.username().is_empty() || url.password().is_some() {
        return Err(LiveHostError::BadRelay(
            "the address must not contain a user name or password".to_owned(),
        ));
    }
    let host = url
        .host_str()
        .filter(|h| !h.is_empty())
        .ok_or_else(|| LiveHostError::BadRelay("the address has no host".to_owned()))?;
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    Ok(match url.port() {
        Some(port) => format!("{scheme}://{host}:{port}"),
        None => format!("{scheme}://{host}"),
    })
}

impl InviteLink {
    /// Render as the string people paste.
    pub fn render(&self) -> Result<String, LiveHostError> {
        let mut url = Url::parse(&format!("{SCHEME}://join"))
            .map_err(|e| LiveHostError::BadInvite(e.to_string()))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("relay", &self.relay);
            if let Some(file) = &self.file {
                query.append_pair("file", file);
            }
        }
        url.set_fragment(Some(&self.fragment));
        Ok(url.into())
    }

    /// Parse a pasted invite.
    pub fn parse(text: &str) -> Result<Self, LiveHostError> {
        let bad = |why: &str| LiveHostError::BadInvite(why.to_owned());
        let url = Url::parse(text.trim()).map_err(|_| bad("it is not a link"))?;
        if url.scheme() != SCHEME {
            return Err(bad("it does not start with manang-live://"));
        }
        let mut relay = None;
        let mut file = None;
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "relay" => relay = Some(value.into_owned()),
                "file" if !value.is_empty() => file = Some(value.into_owned()),
                _ => {}
            }
        }
        let relay = normalize_relay(&relay.ok_or_else(|| bad("it names no relay"))?)
            .map_err(|e| LiveHostError::BadInvite(e.to_string()))?;
        let fragment = url
            .fragment()
            .filter(|f| !f.is_empty())
            .ok_or_else(|| bad("the key is missing"))?
            .to_owned();
        Ok(Self {
            relay,
            file,
            fragment,
        })
    }

    /// The WebSocket URL of a room on this relay.
    #[must_use]
    pub fn room_url(&self, room_hex: &str) -> String {
        room_url(&self.relay, room_hex)
    }
}

/// `<origin>/room/<hex>`.
#[must_use]
pub fn room_url(origin: &str, room_hex: &str) -> String {
    format!("{origin}/room/{room_hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_addresses_are_normalised() {
        assert_eq!(
            normalize_relay("wss://relay.example.org/").expect("ok"),
            "wss://relay.example.org"
        );
        assert_eq!(
            normalize_relay("relay.example.org").expect("bare host means wss"),
            "wss://relay.example.org"
        );
        assert_eq!(
            normalize_relay("http://127.0.0.1:8787/x?y=1").expect("ok"),
            "ws://127.0.0.1:8787"
        );
        assert!(matches!(normalize_relay("  "), Err(LiveHostError::NoRelay)));
        assert!(normalize_relay("ftp://x.org").is_err());
        assert!(normalize_relay("wss://user:pw@x.org").is_err());
    }

    #[test]
    fn invite_round_trips_and_keeps_the_key_in_the_fragment() {
        let link = InviteLink {
            relay: "wss://relay.example.org".to_owned(),
            file: Some("chapters/intro é.typ".to_owned()),
            fragment: "AbC-_123".to_owned(),
        };
        let text = link.render().expect("render");
        assert!(text.starts_with("manang-live://join?"));
        let (before, after) = text.split_once('#').expect("fragment");
        assert_eq!(after, "AbC-_123");
        assert!(
            !before.contains("AbC-_123"),
            "the key must only be in the fragment"
        );
        assert_eq!(InviteLink::parse(&text).expect("parse"), link);
    }

    #[test]
    fn bad_invites_are_errors_not_panics() {
        for text in [
            "",
            "hello",
            "https://example.org/#abc",
            "manang-live://join#abc",
            "manang-live://join?relay=wss%3A%2F%2Fx.org",
            "manang-live://join?relay=ftp%3A%2F%2Fx.org#abc",
        ] {
            assert!(InviteLink::parse(text).is_err(), "{text:?}");
        }
    }
}
