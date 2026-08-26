//! XEP-0215 External Service Discovery.
//!
//! Fetches the account server's STUN/TURN relays so Jingle/`webrtcbin` ICE can traverse NAT.
//! Without a TURN relay, calls fail ICE on restrictive networks (symmetric NAT, UDP-blocking
//! firewalls) where STUN alone can't find a usable candidate pair — this is what monocles
//! Android does too (`IceServers`/`Services`), so it also keeps the two clients on par.

use minidom::Element;

use crate::client::{AccountConfig, Writer};
use crate::xeps::{iq, roster};

const NS_EXTDISCO: &str = "urn:xmpp:extdisco:2";

/// Query the account's server for external services and hand any STUN/TURN relays to the media
/// layer. Best-effort: on failure the media layer keeps its built-in public-STUN fallback.
pub async fn fetch(w: &Writer, cfg: &AccountConfig) {
    let domain = cfg.bare().split('@').nth(1).unwrap_or_default().to_string();
    if domain.is_empty() {
        return;
    }
    let req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "get")
        .attr(crate::ncname("to"), &domain)
        .attr(crate::ncname("id"), roster::new_id("extdisco"))
        .append(Element::builder("services", NS_EXTDISCO).build())
        .build();
    let reply = match iq::request(w, req).await {
        Ok(r) => r,
        Err(e) => {
            tracing::info!(error = %e, "XEP-0215 external services unavailable; using STUN only");
            return;
        }
    };
    let Some(services) = reply.get_child("services", NS_EXTDISCO) else {
        return;
    };

    let mut out = Vec::new();
    for s in services.children().filter(|c| c.name() == "service") {
        let kind = s.attr("type").unwrap_or_default().to_string();
        if !matches!(kind.as_str(), "stun" | "stuns" | "turn" | "turns") {
            continue;
        }
        let host = s.attr("host").unwrap_or_default().to_string();
        let Some(port) = s.attr("port").and_then(|p| p.parse::<u16>().ok()) else {
            continue;
        };
        if host.is_empty() {
            continue;
        }
        let transport = s.attr("transport").unwrap_or("udp").to_string();
        // STUN/TURN over TLS (stuns/turns) requires a TCP transport — skip the invalid combo.
        if matches!(kind.as_str(), "stuns" | "turns") && transport == "udp" {
            continue;
        }
        out.push(mxc_media::IceServer {
            kind,
            host,
            port,
            transport,
            username: s.attr("username").map(str::to_string),
            password: s.attr("password").map(str::to_string),
        });
    }

    if out.is_empty() {
        tracing::info!("XEP-0215 returned no usable ICE servers; using STUN only");
        return;
    }
    mxc_media::set_ice_servers(out);
}
