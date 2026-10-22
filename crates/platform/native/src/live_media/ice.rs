use floe_kernel::AgentFailure;
use std::{collections::HashSet, fmt, net::Ipv6Addr};
use zeroize::Zeroizing;

const MAX_ICE_SERVERS: usize = 8;
const MAX_ICE_URLS_PER_SERVER: usize = 4;
const MAX_ICE_URL_BYTES: usize = 2_048;
const MAX_ICE_USERNAME_BYTES: usize = 512;
const MAX_ICE_CREDENTIAL_BYTES: usize = 4_096;

/// Transport configuration scoped to one process-local native media lease.
/// This value is intentionally not Clone, Serialize, or persisted.
#[derive(Default)]
pub struct LiveIceConfiguration {
    servers: Vec<LiveIceServer>,
}
impl LiveIceConfiguration {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn new(servers: Vec<LiveIceServer>) -> Result<Self, AgentFailure> {
        if servers.len() > MAX_ICE_SERVERS {
            return Err(AgentFailure::InvalidInput);
        }
        let mut seen = HashSet::new();
        for server in &servers {
            for url in &server.urls {
                if !seen.insert(url.as_str()) {
                    return Err(AgentFailure::InvalidInput);
                }
            }
        }
        Ok(Self { servers })
    }

    pub(super) fn servers(&self) -> &[LiveIceServer] {
        &self.servers
    }
}
impl fmt::Debug for LiveIceConfiguration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LiveIceConfiguration")
            .field("server_count", &self.servers.len())
            .field("credentials", &"[redacted]")
            .finish()
    }
}

/// One validated STUN or TURN service. TURN credentials are zeroized when this
/// process-local configuration drops and never implement serialization.
pub struct LiveIceServer {
    urls: Vec<String>,
    username: Option<Zeroizing<String>>,
    credential: Option<Zeroizing<String>>,
}
impl LiveIceServer {
    pub fn new(
        urls: Vec<String>,
        username: Option<String>,
        credential: Option<String>,
    ) -> Result<Self, AgentFailure> {
        let username = username.map(Zeroizing::new);
        let credential = credential.map(Zeroizing::new);
        if urls.is_empty()
            || urls.len() > MAX_ICE_URLS_PER_SERVER
            || urls.iter().any(|url| valid_ice_url(url).is_none())
        {
            return Err(AgentFailure::InvalidInput);
        }
        match (username.as_deref(), credential.as_deref()) {
            (Some(user), Some(secret))
                if valid_secret(user, MAX_ICE_USERNAME_BYTES)
                    && valid_secret(secret, MAX_ICE_CREDENTIAL_BYTES)
                    && urls
                        .iter()
                        .all(|url| matches!(valid_ice_url(url), Some("turn" | "turns"))) => {}
            (None, None) => {}
            _ => return Err(AgentFailure::InvalidInput),
        }
        Ok(Self {
            urls,
            username,
            credential,
        })
    }

    pub(super) fn urls(&self) -> &[String] {
        &self.urls
    }

    pub(super) fn username(&self) -> Option<&str> {
        self.username.as_deref().map(String::as_str)
    }

    pub(super) fn credential(&self) -> Option<&str> {
        self.credential.as_deref().map(String::as_str)
    }
}
impl fmt::Debug for LiveIceServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LiveIceServer")
            .field("url_count", &self.urls.len())
            .field("credentials", &"[redacted]")
            .finish()
    }
}

fn valid_secret(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.chars().all(|character| !character.is_control())
}

fn valid_ice_url(url: &str) -> Option<&str> {
    if url.is_empty()
        || url.len() > MAX_ICE_URL_BYTES
        || !url.is_ascii()
        || url.chars().any(char::is_whitespace)
    {
        return None;
    }
    let (scheme, remainder) = url.split_once(':')?;
    if !matches!(scheme, "stun" | "stuns" | "turn" | "turns") {
        return None;
    }
    let (authority, query) = match remainder.split_once('?') {
        Some((authority, query)) => (authority, Some(query)),
        None => (remainder, None),
    };
    if authority.is_empty()
        || authority.starts_with("//")
        || authority.chars().any(|character| {
            matches!(character, '/' | '\\' | '@' | '#' | '%' | '?' | '\0') || character.is_control()
        })
        || !valid_authority(authority)
    {
        return None;
    }
    if let Some(query) = query {
        let valid_transport = matches!(query, "transport=udp" | "transport=tcp")
            && matches!(scheme, "turn" | "turns")
            && !(scheme == "turns" && query == "transport=udp");
        if !valid_transport {
            return None;
        }
    }
    Some(scheme)
}

fn valid_authority(authority: &str) -> bool {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((address, remainder)) = bracketed.split_once(']') else {
            return false;
        };
        if address.parse::<Ipv6Addr>().is_err() {
            return false;
        }
        if remainder.is_empty() {
            return true;
        }
        return remainder.strip_prefix(':').is_some_and(valid_port);
    }

    let mut parts = authority.split(':');
    let Some(host) = parts.next() else {
        return false;
    };
    let port = parts.next();
    if parts.next().is_some() || !valid_hostname(host) {
        return false;
    }
    port.is_none_or(valid_port)
}

fn valid_hostname(host: &str) -> bool {
    if host.is_empty() || host.len() > 253 {
        return false;
    }
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn valid_port(port: &str) -> bool {
    !port.is_empty()
        && port.bytes().all(|byte| byte.is_ascii_digit())
        && port.parse::<u16>().is_ok_and(|value| value > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_transport_urls_and_redacts_ephemeral_turn_credentials() {
        let stun =
            LiveIceServer::new(vec!["stun:stun.example.test:3478".into()], None, None).unwrap();
        let turn = LiveIceServer::new(
            vec!["turn:turn.example.test:3478?transport=udp".into()],
            Some("fixture-username".into()),
            Some("fixture-ephemeral-credential".into()),
        )
        .unwrap();
        let configuration = LiveIceConfiguration::new(vec![stun, turn]).unwrap();
        let debug = format!("{configuration:?}");
        assert!(debug.contains("server_count: 2"));
        assert!(!debug.contains("fixture-username"));
        assert!(!debug.contains("fixture-ephemeral-credential"));
        assert_eq!(LiveIceConfiguration::empty().servers().len(), 0);
    }

    #[test]
    fn rejects_invalid_or_ambiguous_server_configuration() {
        for url in [
            "https://ice.example.test",
            "stun://user@ice.example.test",
            "stun:ice.example.test:0",
            "stun:ice.example.test:65536",
            "turn:ice.example.test?credential=inline",
            "turns:ice.example.test?transport=udp",
            "turn:ice.example.test?transport=other",
            "stun:[not-ipv6]:3478",
            "stun:ice.example.test/path",
        ] {
            assert!(
                LiveIceServer::new(vec![url.into()], None, None).is_err(),
                "accepted invalid test URL: {url}"
            );
        }
        assert!(
            LiveIceServer::new(
                vec!["stun:ice.example.test".into()],
                Some("fixture-user".into()),
                Some("fixture-secret".into()),
            )
            .is_err()
        );
        assert!(
            LiveIceServer::new(
                vec!["turn:ice.example.test".into()],
                Some("fixture-user".into()),
                None,
            )
            .is_err()
        );
        assert!(
            LiveIceServer::new(
                vec!["turn:ice.example.test".into()],
                None,
                Some("fixture-secret".into()),
            )
            .is_err()
        );
        let server = LiveIceServer::new(vec!["stun:ice.example.test".into()], None, None).unwrap();
        assert!(
            LiveIceConfiguration::new(vec![
                server,
                LiveIceServer::new(vec!["stun:ice.example.test".into()], None, None,).unwrap()
            ])
            .is_err()
        );
    }
}
