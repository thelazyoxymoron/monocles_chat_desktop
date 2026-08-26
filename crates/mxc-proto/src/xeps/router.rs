//! Top-level stanza demultiplexer: routes an incoming `<message>`/`<presence>`/`<iq>`
//! to the right handler. Awaited iq *replies* are intercepted earlier by
//! [`super::iq::try_resolve`] in the reader loop, so here we only see requests/pushes.

use async_channel::Sender;
use minidom::Element;

use mxc_store::Store;

use crate::client::{AccountConfig, Writer};
use crate::event::Event;
use crate::xeps::jingle::CallRegistry;
use crate::xeps::{disco, jingle, messaging, presence, roster};

pub async fn handle_stanza(
    w: &Writer,
    store: &Store,
    cfg: &AccountConfig,
    events: &Sender<Event>,
    calls: &CallRegistry,
    stanza: Element,
) -> anyhow::Result<()> {
    match stanza.name() {
        "message" => messaging::handle_incoming(w, store, cfg, events, calls, &stanza).await,
        "presence" => {
            // XEP-0272 Muji: drive any active group call off occupant `<muji>` presence.
            jingle::observe_muji_presence(w, calls, cfg, events, &stanza).await;
            presence::handle_incoming(w, store, cfg, events, &stanza).await
        }
        "iq" => handle_iq(w, store, cfg, events, calls, &stanza).await,
        other => {
            tracing::trace!(other, "ignoring unknown top-level stanza");
            Ok(())
        }
    }
}

async fn handle_iq(
    w: &Writer,
    store: &Store,
    cfg: &AccountConfig,
    events: &Sender<Event>,
    calls: &CallRegistry,
    iq: &Element,
) -> anyhow::Result<()> {
    let iq_type = iq.attr("type").unwrap_or("");

    // XEP-0166 Jingle session IQs (calls).
    if iq.get_child("jingle", jingle::NS_JINGLE_SESSION).is_some()
        && jingle::handle_iq(w, calls, cfg, events, iq).await
    {
        return Ok(());
    }

    // Roster pushes (set) / results.
    if let Some(query) = iq.get_child("query", "jabber:iq:roster") {
        roster::handle_roster_payload(store, cfg, events, query).await?;
        if iq_type == "set" {
            roster::ack_iq(w, iq)?;
        }
        return Ok(());
    }

    // disco#info / disco#items requests against us.
    if iq_type == "get" {
        if iq.get_child("query", "http://jabber.org/protocol/disco#info").is_some() {
            disco::answer_info(w, iq)?;
            return Ok(());
        }
        if iq.get_child("query", "http://jabber.org/protocol/disco#items").is_some() {
            disco::answer_items(w, iq)?;
            return Ok(());
        }
    }

    tracing::trace!(iq_type, "unhandled iq");
    Ok(())
}
