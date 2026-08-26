//! XEP-0280 Message Carbons: mirror messages across our own devices.

use minidom::Element;

use crate::client::Writer;
use crate::xeps::roster::new_id;

pub const NS_CARBONS: &str = "urn:xmpp:carbons:2";
pub const NS_FORWARD: &str = "urn:xmpp:forward:0";

/// Whether a carbon wrapper indicates the inner message was sent by us (vs. received).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarbonKind {
    Sent,
    Received,
}

/// Enable carbons for this session (after bind).
pub fn enable(w: &Writer) -> anyhow::Result<()> {
    let iq = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "set")
        .attr(crate::ncname("id"), new_id("carbons"))
        .append(Element::builder("enable", NS_CARBONS).build())
        .build();
    // fire-and-forget; a failure just means no multi-device mirroring.
    w.send(iq)
}

/// If `msg` is a carbon, return the unwrapped inner `<message>` and its direction.
///
/// Security: per XEP-0280 we MUST only trust carbons whose outer `from` is our own bare
/// JID; the caller enforces that with `our_bare`.
pub fn unwrap(msg: &Element, our_bare: &str) -> Option<(Element, CarbonKind)> {
    let outer_from = msg.attr("from").unwrap_or("");
    let outer_bare = outer_from.split('/').next().unwrap_or(outer_from);
    if !outer_bare.eq_ignore_ascii_case(our_bare) {
        return None;
    }

    for (child_name, kind) in [("sent", CarbonKind::Sent), ("received", CarbonKind::Received)] {
        if let Some(wrap) = msg.get_child(child_name, NS_CARBONS) {
            if let Some(fwd) = wrap.get_child("forwarded", NS_FORWARD) {
                if let Some(inner) = fwd.get_child("message", "jabber:client") {
                    return Some((inner.clone(), kind));
                }
            }
        }
    }
    None
}
