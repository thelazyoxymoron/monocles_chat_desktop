//! XEP-0272 Muji — multiparty Jingle (group calls).
//!
//! A Muji conference *is* a MUC. Participants coordinate through their MUC presence: each
//! announces a `<muji>` payload (first `<preparing/>`, then the media `<content>` codec
//! advertisement) and then establishes a **full mesh** of ordinary 1:1 Jingle RTP sessions —
//! one per other participant — each tagged with `<muji room=…/>`. This module owns only the
//! Muji *presence* coordination + the glare tie-break; the per-pair sessions reuse the 1:1
//! machinery in [`super::jingle`] / [`super::jingle_sdp`].

use minidom::Element;

use crate::client::Writer;
use crate::xeps::{caps, jingle_sdp};

/// XEP-0272 Muji namespace.
pub const NS_MUJI: &str = "urn:xmpp:jingle:muji:0";

/// A peer occupant's advertised Muji state, parsed from their MUC presence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MujiState {
    /// `<muji><preparing/></muji>` — allocating streams, not yet callable.
    Preparing,
    /// `<muji>` with `<content>` and no `<preparing/>` — ready to be called.
    Ready,
}

/// Build our `<muji>` presence payload. While `prepared` is false we advertise only
/// `<preparing/>`; once true we advertise the media `<content>` codec set (Opus audio, plus
/// VP8 video when `video`) and drop `<preparing/>`. The payload-types are advisory (XEP-0272
/// §codec coordination) — our engine's set is fixed, and the real per-pair sessions still run
/// a full SDP offer/answer.
///
/// `device` is our OMEMO device id, advertised as the `device` attribute (wire-compatible with
/// the monocles/Conversations Android client): a peer reads it to know which device to OMEMO-
/// encrypt its per-pair DTLS fingerprint to. Without it, an Android peer with required OMEMO
/// verification cannot encrypt to us and refuses the leg.
pub fn muji_payload(prepared: bool, video: bool, device: Option<u32>) -> Element {
    let mut muji = Element::builder("muji", NS_MUJI);
    if let Some(dev) = device {
        muji = muji.attr(crate::ncname("device"), dev.to_string());
    }
    if !prepared {
        return muji.append(Element::builder("preparing", NS_MUJI).build()).build();
    }
    // Audio content (Opus).
    let audio = Element::builder("content", NS_MUJI)
        .attr(crate::ncname("creator"), "initiator")
        .attr(crate::ncname("name"), "voice")
        .append(
            Element::builder("description", jingle_sdp::NS_RTP)
                .attr(crate::ncname("media"), "audio")
                .append(
                    Element::builder("payload-type", jingle_sdp::NS_RTP)
                        .attr(crate::ncname("id"), "111")
                        .attr(crate::ncname("name"), "opus")
                        .attr(crate::ncname("clockrate"), "48000")
                        .attr(crate::ncname("channels"), "2")
                        .build(),
                )
                .build(),
        )
        .build();
    muji = muji.append(audio);
    if video {
        let vid = Element::builder("content", NS_MUJI)
            .attr(crate::ncname("creator"), "initiator")
            .attr(crate::ncname("name"), "webcam")
            .append(
                Element::builder("description", jingle_sdp::NS_RTP)
                    .attr(crate::ncname("media"), "video")
                    .append(
                        Element::builder("payload-type", jingle_sdp::NS_RTP)
                            .attr(crate::ncname("id"), "96")
                            .attr(crate::ncname("name"), "VP8")
                            .attr(crate::ncname("clockrate"), "90000")
                            .build(),
                    )
                    .build(),
            )
            .build();
        muji = muji.append(vid);
    }
    muji.build()
}

/// Send a directed MUC presence to `room/nick` carrying an optional `<muji>` payload (plus our
/// caps). `muji = None` re-sends a plain presence with no `<muji>` — i.e. we have left the
/// conference (XEP-0272: drop `<muji>` first, *then* terminate the Jingle sessions).
pub fn send_muji_presence(
    w: &Writer,
    room: &str,
    nick: &str,
    muji: Option<Element>,
) -> anyhow::Result<()> {
    let mut pres = Element::builder("presence", "jabber:client")
        .attr(crate::ncname("to"), format!("{room}/{nick}"))
        .append(caps::caps_element());
    if let Some(m) = muji {
        pres = pres.append(m);
    }
    w.send(pres.build())
}

/// Inspect an occupant's presence for a `<muji>` element and classify it. Returns `None` when
/// there is no `<muji>` (the occupant is not participating, or has left the conference).
pub fn parse_muji_state(presence: &Element) -> Option<MujiState> {
    let muji = presence.get_child("muji", NS_MUJI)?;
    if muji.get_child("preparing", NS_MUJI).is_some() {
        Some(MujiState::Preparing)
    } else if muji.children().any(|c| c.name() == "content") {
        Some(MujiState::Ready)
    } else {
        // A bare `<muji/>` with neither preparing nor content — treat as preparing.
        Some(MujiState::Preparing)
    }
}

/// Glare tie-break (XEP-0272 §6): when two ready participants could each initiate a Jingle
/// session with the other, only one must. Deterministically pick the participant whose
/// occupant JID (`room/nick`) is lexicographically greater as the initiator. Both sides see the
/// same pair of occupant JIDs, so exactly one returns `true`.
pub fn should_initiate(my_occupant_jid: &str, their_occupant_jid: &str) -> bool {
    my_occupant_jid > their_occupant_jid
}
