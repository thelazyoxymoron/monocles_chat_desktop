//! WebXDC (`urn:xmpp:webxdc:0`) — shared mini-apps, with status-update + realtime syncing.
//!
//! A `.xdc` (zip) shared in a chat is an app instance, keyed by the `<thread>` UUID on its file
//! message. Participants then exchange **status updates** — `<message>`s carrying
//! `<x xmlns='urn:xmpp:webxdc:0'>` with a `<json xmlns='urn:xmpp:json:0'>` payload (+ optional
//! `<document>`/`<summary>`, and the message body as `info`) and the same `<thread>` — which all
//! clients append to the instance's update log and feed to the running app. Ephemeral **realtime**
//! data rides the same envelope as `<data>` (base64) and isn't stored.
//!
//! The actual stanza building (encryption-aware) lives in [`super::messaging`]; this module owns
//! the namespaces, element (de)serialization, and the incoming side-effect (store + notify).

use async_channel::Sender;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use minidom::Element;

use mxc_store::Store;

use crate::client::AccountConfig;
use crate::event::Event;

pub const NS_WEBXDC: &str = "urn:xmpp:webxdc:0";
pub const NS_JSON: &str = "urn:xmpp:json:0";
const NS_CLIENT: &str = "jabber:client";

/// Build the `<x xmlns='urn:xmpp:webxdc:0'>` for a status update (any of the parts may be absent).
/// `notify` is the WebXDC notification dict (selfAddr → text, plus `"*"`) serialized as JSON; it
/// rides verbatim in a `<notify>` element so recipients can selectively notify the right user.
pub fn build_update_x(
    payload: Option<&str>,
    document: Option<&str>,
    summary: Option<&str>,
    notify: Option<&str>,
) -> Element {
    let mut x = Element::builder("x", NS_WEBXDC);
    if let Some(p) = payload {
        x = x.append(Element::builder("json", NS_JSON).append(p).build());
    }
    if let Some(d) = document {
        x = x.append(Element::builder("document", NS_WEBXDC).append(d).build());
    }
    if let Some(s) = summary {
        x = x.append(Element::builder("summary", NS_WEBXDC).append(s).build());
    }
    if let Some(n) = notify.filter(|s| !s.is_empty()) {
        x = x.append(Element::builder("notify", NS_WEBXDC).append(n).build());
    }
    x.build()
}

/// Resolve the WebXDC notification text *this* user should see from a `notify` dict (JSON of
/// `selfAddr → text`). Matches our identity in the forms an app might key by — the `xmpp:` URI,
/// the bare JID, or the `"*"` catch-all — returning the first match's text.
pub fn notify_text_for(notify_json: &str, bare_jid: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(notify_json).ok()?;
    let obj = value.as_object()?;
    for key in [format!("xmpp:{bare_jid}"), bare_jid.to_string(), "*".to_string()] {
        if let Some(text) = obj.get(&key).and_then(|v| v.as_str()) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

/// Build the `<x xmlns='urn:xmpp:webxdc:0'><data>…</data></x>` for ephemeral realtime data.
pub fn build_realtime_x(data_b64: &str) -> Element {
    Element::builder("x", NS_WEBXDC)
        .append(Element::builder("data", NS_WEBXDC).append(data_b64).build())
        .build()
}

/// Build the `<thread>` element that ties an update to its app instance.
pub fn thread_element(thread: &str) -> Element {
    Element::builder("thread", NS_CLIENT).append(thread).build()
}

/// Render a stored update as the JSON object the in-app `getStatusUpdates` API returns. `payload`
/// is already-serialized JSON; the rest are quoted. `serial`/`max_serial` page the JS cursor.
pub fn update_json(
    serial: i64,
    max_serial: i64,
    sender: Option<&str>,
    info: Option<&str>,
    document: Option<&str>,
    summary: Option<&str>,
    payload: Option<&str>,
) -> String {
    let mut s = String::from("{");
    s.push_str(&format!("\"serial\":{serial},\"max_serial\":{max_serial}"));
    if let Some(v) = sender {
        s.push_str(&format!(",\"sender\":{}", json_quote(v)));
    }
    if let Some(v) = info {
        s.push_str(&format!(",\"info\":{}", json_quote(v)));
    }
    if let Some(v) = document {
        s.push_str(&format!(",\"document\":{}", json_quote(v)));
    }
    if let Some(v) = summary {
        s.push_str(&format!(",\"summary\":{}", json_quote(v)));
    }
    if let Some(v) = payload {
        s.push_str(&format!(",\"payload\":{v}"));
    }
    s.push('}');
    s
}

/// Minimal JSON string quoting (escapes the control + structural characters).
pub fn json_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Handle an incoming WebXDC `<x>` for `thread`: store a status update (+ notify so an open app
/// view replays it), or forward realtime `<data>` directly. `sender` is the real bare JID.
pub async fn handle_incoming_update(
    store: &Store,
    cfg: &AccountConfig,
    events: &Sender<Event>,
    thread: &str,
    sender: &str,
    message_id: Option<&str>,
    x: &Element,
    info: Option<&str>,
) -> anyhow::Result<()> {
    // Ephemeral realtime data is not stored — just forwarded to a live app view.
    if let Some(data) = x.get_child("data", NS_WEBXDC).map(|d| d.text()) {
        if !data.is_empty() {
            let _ = events
                .send(Event::WebxdcRealtime {
                    account_id: cfg.account_id,
                    thread: thread.to_string(),
                    data_b64: data,
                })
                .await;
            return Ok(());
        }
    }

    // WebXDC notification API: the app can ask for a user-visible notification, selectively per
    // recipient. Notify only if WE are addressed (our id or `"*"`) and the update isn't our own.
    if !sender.eq_ignore_ascii_case(cfg.bare()) {
        if let Some(notify) = x.get_child("notify", NS_WEBXDC).map(|e| e.text()).filter(|s| !s.is_empty()) {
            if let Some(text) = notify_text_for(&notify, cfg.bare()) {
                let _ = events
                    .send(Event::WebxdcNotify {
                        account_id: cfg.account_id,
                        thread: thread.to_string(),
                        text,
                    })
                    .await;
            }
        }
    }

    let document = x.get_child("document", NS_WEBXDC).map(|e| e.text()).filter(|s| !s.is_empty());
    let summary = x.get_child("summary", NS_WEBXDC).map(|e| e.text()).filter(|s| !s.is_empty());
    let payload = x.get_child("json", NS_JSON).map(|e| e.text()).filter(|s| !s.is_empty());
    let info = info.filter(|s| !s.is_empty());
    if document.is_none() && summary.is_none() && payload.is_none() && info.is_none() {
        return Ok(());
    }
    let serial = store
        .insert_webxdc_update(
            cfg.account_id,
            thread,
            message_id,
            Some(sender),
            info,
            document.as_deref(),
            summary.as_deref(),
            payload.as_deref(),
        )
        .await?;
    let _ = events
        .send(Event::WebxdcUpdate {
            account_id: cfg.account_id,
            thread: thread.to_string(),
            serial,
        })
        .await;
    Ok(())
}

/// Encode raw bytes as base64 (for realtime `<data>`), used by the send path.
pub fn b64(data: &[u8]) -> String {
    B64.encode(data)
}

/// Decode standard base64 (used for `sendToChat` file payloads); `None` if malformed.
pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    B64.decode(s.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_json_shape() {
        // payload is raw (already-serialized) JSON; the rest are quoted. Field names match the
        // webxdc.js / monocles Android update object.
        let j = update_json(
            3,
            5,
            Some("xmpp:a@b"),
            Some("Alice moved"),
            None,
            Some("score 3"),
            Some(r#"{"move":3}"#),
        );
        assert_eq!(
            j,
            r#"{"serial":3,"max_serial":5,"sender":"xmpp:a@b","info":"Alice moved","summary":"score 3","payload":{"move":3}}"#
        );
        // No payload / metadata → just the cursor fields.
        assert_eq!(update_json(1, 1, None, None, None, None, None), r#"{"serial":1,"max_serial":1}"#);
    }

    #[test]
    fn json_quote_escapes() {
        assert_eq!(json_quote("a\"b\\c\n"), "\"a\\\"b\\\\c\\n\"");
    }

    #[test]
    fn round_trip_update_x() {
        let x = build_update_x(Some(r#"{"k":1}"#), Some("doc"), Some("sum"), Some(r#"{"*":"hi"}"#));
        let xml = String::from(&x);
        let parsed: Element = xml.parse().unwrap();
        assert_eq!(parsed.get_child("json", NS_JSON).map(|e| e.text()).as_deref(), Some(r#"{"k":1}"#));
        assert_eq!(parsed.get_child("document", NS_WEBXDC).map(|e| e.text()).as_deref(), Some("doc"));
        assert_eq!(parsed.get_child("summary", NS_WEBXDC).map(|e| e.text()).as_deref(), Some("sum"));
        assert_eq!(parsed.get_child("notify", NS_WEBXDC).map(|e| e.text()).as_deref(), Some(r#"{"*":"hi"}"#));
    }

    #[test]
    fn notify_matches_identity() {
        let j = r#"{"xmpp:bob@x":"your turn","*":"updated"}"#;
        assert_eq!(notify_text_for(j, "bob@x").as_deref(), Some("your turn"));
        assert_eq!(notify_text_for(j, "carol@y").as_deref(), Some("updated")); // "*" catch-all
        assert_eq!(notify_text_for(r#"{"xmpp:bob@x":"hi"}"#, "carol@y"), None); // not addressed
    }
}
