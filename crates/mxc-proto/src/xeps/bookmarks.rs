//! XEP-0402 Bookmarks2 (PEP node `urn:xmpp:bookmarks:1`): fetch + persist MUC bookmarks.
//!
//! Each PEP item id is the room bare JID; the payload is a `<conference>` with optional
//! `name`, `autojoin`, and `<nick>`. We mirror them into `conversations` (kind='muc').

use mxc_store::Store;

use crate::client::{AccountConfig, Writer};
use crate::xeps::pep;

pub const NS_BOOKMARKS: &str = "urn:xmpp:bookmarks:1";
pub const NODE: &str = "urn:xmpp:bookmarks:1";

/// Fetch all bookmarks and upsert them as MUC conversations.
pub async fn fetch(w: &Writer, store: &Store, cfg: &AccountConfig) -> anyhow::Result<()> {
    let reply = pep::items(w, None, NODE, None).await?;
    for (item_id, payload) in pep::extract_items(&reply) {
        let Some(room) = item_id else { continue };
        if payload.name() != "conference" {
            continue;
        }
        let name = payload.attr("name").map(str::to_string);
        let autojoin = matches!(payload.attr("autojoin"), Some("true") | Some("1"));
        let nick = payload.get_child("nick", NS_BOOKMARKS).map(|n| n.text());
        store
            .upsert_muc(cfg.account_id, &room, name.as_deref(), nick.as_deref(), autojoin)
            .await?;
    }
    Ok(())
}

/// Add/replace a bookmark (used when the user joins a new room and opts to remember it).
pub async fn save(
    w: &Writer,
    room: &str,
    name: Option<&str>,
    nick: Option<&str>,
    autojoin: bool,
) -> anyhow::Result<()> {
    use minidom::Element;
    let mut conf = Element::builder("conference", NS_BOOKMARKS)
        .attr(crate::ncname("autojoin"), if autojoin { "true" } else { "false" });
    if let Some(n) = name {
        conf = conf.attr(crate::ncname("name"), n);
    }
    if let Some(n) = nick {
        conf = conf.append(Element::builder("nick", NS_BOOKMARKS).append(n).build());
    }
    // bookmarks2 wants an open access model so other clients can read them.
    pep::publish(w, NODE, Some(room), conf.build(), Some(pep::publish_options("whitelist"))).await?;
    Ok(())
}

/// Remove a bookmark (XEP-0402 retract), used when the user leaves a room.
pub async fn remove(w: &Writer, room: &str) -> anyhow::Result<()> {
    pep::retract(w, NODE, room).await?;
    Ok(())
}
