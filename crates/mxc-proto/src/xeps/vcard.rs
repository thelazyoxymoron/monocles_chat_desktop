//! Minimal vcard-temp (XEP-0054) photo fetch, used for MUC occupant avatars (XEP-0153).
//!
//! MUC participants are often semi-anonymous, so their avatars can't be read from PEP
//! (XEP-0084) by real JID. Instead we fetch the occupant's `vcard-temp` from the room
//! (`room/nick`) and pull the `<PHOTO><BINVAL>` image bytes.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use minidom::Element;

use crate::client::Writer;
use crate::xeps::{avatar, iq, pep};
use crate::xeps::roster::new_id;

const NS_VCARD: &str = "vcard-temp";
/// PEP vCard4 (XEP-0292): the PubSub node and the vCard4 element namespace.
const NODE_VCARD4: &str = "urn:xmpp:vcard4";
const NS_VCARD4: &str = "urn:ietf:params:xml:ns:vcard-4.0";

/// Canonical display order for merged profile fields (across vcard-temp, vCard4, PEP nick).
const FIELD_ORDER: &[&str] = &[
    "Name", "Nickname", "Title", "Role", "Organization", "Email", "Phone", "Website",
    "Birthday", "About",
];

/// Fetch `jid`'s vCard photo bytes (`None` if it has no photo or the request fails). For a
/// MUC occupant, `jid` is the full `room/nick`.
pub async fn fetch_photo(w: &Writer, jid: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "get")
        .attr(crate::ncname("to"), jid)
        .attr(crate::ncname("id"), new_id("vcard"))
        .append(Element::builder("vCard", NS_VCARD).build())
        .build();
    let reply = iq::request(w, req).await?;

    let Some(vcard) = reply.get_child("vCard", NS_VCARD) else { return Ok(None) };
    let Some(photo) = vcard.get_child("PHOTO", NS_VCARD) else { return Ok(None) };
    let Some(binval) = photo.get_child("BINVAL", NS_VCARD) else { return Ok(None) };

    // BINVAL is base64 (often line-wrapped); strip whitespace before decoding.
    let b64: String = binval.text().chars().filter(|c| !c.is_whitespace()).collect();
    match B64.decode(b64.as_bytes()) {
        Ok(bytes) if !bytes.is_empty() => Ok(Some(bytes)),
        _ => Ok(None),
    }
}

/// A contact's / room's vCard profile: the photo plus human-readable `(label, value)` fields
/// for display in the profile dialog.
#[derive(Debug, Clone, Default)]
pub struct VcardDetails {
    pub photo: Option<Vec<u8>>,
    /// Ordered label→value pairs (e.g. "Name" → "Alice"), only for fields the vCard carries.
    pub fields: Vec<(String, String)>,
}

/// Fetch `jid`'s full profile (photo + common fields), merging three sources in precedence
/// order: vcard-temp (XEP-0054) first, then PEP vCard4 (XEP-0292), then PEP nickname
/// (XEP-0172) / PEP avatar (XEP-0084) as last-resort fallbacks. For a MUC room, `jid` is the
/// bare room JID; rooms commonly expose `FN`/`DESC` via vcard-temp.
pub async fn fetch_details(w: &Writer, jid: &str) -> anyhow::Result<VcardDetails> {
    // First-wins map keyed by display label, populated in precedence order.
    let mut fields: std::collections::HashMap<&'static str, String> = std::collections::HashMap::new();
    let mut photo: Option<Vec<u8>> = None;

    // --- 1. vcard-temp (primary) ---
    if let Some(vcard) = fetch_vcard_temp(w, jid).await {
        if let Some(b) = vcard
            .get_child("PHOTO", NS_VCARD)
            .and_then(|p| p.get_child("BINVAL", NS_VCARD))
        {
            let b64: String = b.text().chars().filter(|c| !c.is_whitespace()).collect();
            if let Ok(bytes) = B64.decode(b64.as_bytes()) {
                if !bytes.is_empty() {
                    photo = Some(bytes);
                }
            }
        }
        let text_of = |tag: &str| vcard.get_child(tag, NS_VCARD).map(|e| e.text()).unwrap_or_default();
        put(&mut fields, "Name", &text_of("FN"));
        put(&mut fields, "Nickname", &text_of("NICKNAME"));
        put(&mut fields, "Title", &text_of("TITLE"));
        put(&mut fields, "Role", &text_of("ROLE"));
        if let Some(org) = vcard.get_child("ORG", NS_VCARD) {
            put(&mut fields, "Organization", &org.get_child("ORGNAME", NS_VCARD).map(|e| e.text()).unwrap_or_default());
        }
        if let Some(email) = vcard.get_child("EMAIL", NS_VCARD) {
            let v = email.get_child("USERID", NS_VCARD).map(|e| e.text()).unwrap_or_else(|| email.text());
            put(&mut fields, "Email", &v);
        }
        if let Some(tel) = vcard.get_child("TEL", NS_VCARD) {
            let v = tel.get_child("NUMBER", NS_VCARD).map(|e| e.text()).unwrap_or_else(|| tel.text());
            put(&mut fields, "Phone", &v);
        }
        put(&mut fields, "Website", &text_of("URL"));
        put(&mut fields, "Birthday", &text_of("BDAY"));
        put(&mut fields, "About", &text_of("DESC"));
    }

    // --- 2. PEP vCard4 / XEP-0292 (fallback for any field vcard-temp lacked) ---
    if let Some(v4) = fetch_vcard4(w, jid).await {
        // vCard4 wraps each value, e.g. <fn><text>Name</text></fn>, <tel><uri>tel:…</uri></tel>.
        let val = |tag: &str, inner: &str| -> String {
            v4.get_child(tag, NS_VCARD4)
                .and_then(|e| e.get_child(inner, NS_VCARD4))
                .map(|e| e.text())
                .unwrap_or_default()
        };
        put(&mut fields, "Name", &val("fn", "text"));
        put(&mut fields, "Nickname", &val("nickname", "text"));
        put(&mut fields, "Title", &val("title", "text"));
        put(&mut fields, "Role", &val("role", "text"));
        put(&mut fields, "Organization", &val("org", "text"));
        put(&mut fields, "Email", &val("email", "text"));
        // tel/url use <uri>; strip a leading "tel:" scheme for display.
        let tel = val("tel", "uri");
        put(&mut fields, "Phone", tel.strip_prefix("tel:").unwrap_or(&tel));
        put(&mut fields, "Website", &val("url", "uri"));
        put(&mut fields, "Birthday", &val("bday", "date"));
        put(&mut fields, "About", &val("note", "text"));
    }

    // --- 3. PEP nickname (XEP-0172) ---
    if !fields.contains_key("Nickname") {
        if let Ok(reply) = pep::items(w, Some(jid), avatar::NODE_NICK, Some(1)).await {
            if let Some((_, payload)) = pep::extract_items(&reply).into_iter().next() {
                if payload.name() == "nick" {
                    put(&mut fields, "Nickname", &payload.text());
                }
            }
        }
    }

    // --- 4. PEP avatar (XEP-0084) photo, if neither vcard carried one ---
    if photo.is_none() {
        photo = avatar::pep_photo(w, jid).await.ok().flatten();
    }

    // Emit in canonical display order.
    let ordered = FIELD_ORDER
        .iter()
        .filter_map(|&label| fields.remove(label).map(|v| (label.to_string(), v)))
        .collect();
    Ok(VcardDetails { photo, fields: ordered })
}

/// Insert a trimmed, non-empty `value` for `label` only if not already present (first-wins,
/// so earlier higher-precedence sources keep their value).
fn put(map: &mut std::collections::HashMap<&'static str, String>, label: &'static str, value: &str) {
    let value = value.trim();
    if !value.is_empty() {
        map.entry(label).or_insert_with(|| value.to_string());
    }
}

/// Fetch and return the `<vCard>` element from `jid`'s vcard-temp, or `None` on failure.
async fn fetch_vcard_temp(w: &Writer, jid: &str) -> Option<Element> {
    let req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "get")
        .attr(crate::ncname("to"), jid)
        .attr(crate::ncname("id"), new_id("vcard"))
        .append(Element::builder("vCard", NS_VCARD).build())
        .build();
    let reply = iq::request(w, req).await.ok()?;
    reply.get_child("vCard", NS_VCARD).cloned()
}

/// Fetch and return the PEP vCard4 `<vcard>` element for `jid`, or `None` if not published.
async fn fetch_vcard4(w: &Writer, jid: &str) -> Option<Element> {
    let reply = pep::items(w, Some(jid), NODE_VCARD4, Some(1)).await.ok()?;
    pep::extract_items(&reply)
        .into_iter()
        .map(|(_, payload)| payload)
        .find(|p| p.name() == "vcard")
}
