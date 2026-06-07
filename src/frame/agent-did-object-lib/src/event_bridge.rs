use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventBridgeKey {
    pub adapter_id: String,
    pub object: String,
    pub event: String,
    pub filter_hash: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeState {
    Connecting,
    Subscribing,
    Active,
    Renewing,
    Closing,
    Closed,
    Failed,
}

pub fn encode_object_event_id(object: &str, event: &str) -> String {
    let event = safe_segment(event);
    if let Ok(url) = Url::parse(object) {
        if let Some(host) = url.host_str() {
            let mut segments = url
                .path_segments()
                .map(|segments| {
                    segments
                        .filter(|segment| !segment.trim().is_empty())
                        .map(safe_segment)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if segments.is_empty() {
                segments.push("by_hash".to_string());
                segments.push(short_hash(object));
            }
            return format!(
                "/obj/{}/{}/{}",
                safe_segment(&host.to_lowercase()),
                segments.join("/"),
                event
            );
        }
    }
    format!("/obj/by_hash/{}/{}", short_hash(object), event)
}

fn safe_segment(input: &str) -> String {
    let mut value = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '%' {
            let first = chars.peek().copied();
            if first.is_some_and(|value| value.is_ascii_hexdigit()) {
                chars.next();
                let second = chars.peek().copied();
                if second.is_some_and(|value| value.is_ascii_hexdigit()) {
                    chars.next();
                }
            }
            value.push('_');
        } else if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            value.push(ch);
        } else {
            value.push('_');
        }
    }
    if value.is_empty() {
        "_".to_string()
    } else {
        value
    }
}

fn short_hash(input: &str) -> String {
    let hash = Sha256::digest(input.as_bytes());
    hash.iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_url_path_segments() {
        assert_eq!(
            encode_object_event_id("https://MyHome.com/devices/cam 01", "low battery"),
            "/obj/myhome.com/devices/cam_01/low_battery"
        );
    }

    #[test]
    fn falls_back_to_hash_for_empty_path() {
        let eventid = encode_object_event_id("https://example.com", "changed");
        assert!(eventid.starts_with("/obj/example.com/by_hash/"));
        assert!(eventid.ends_with("/changed"));
    }
}
