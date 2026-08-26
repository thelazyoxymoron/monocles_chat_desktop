//! XEP-0313 Message Archive Management — paged history backfill.
//!
//! Issues a MAM query with an RSM `<before>` cursor for a conversation. The matched
//! archive messages arrive as separate `<message><result><forwarded>…` stanzas which the
//! reader loop routes through [`super::messaging::handle_incoming`] (it unwraps the MAM
//! envelope + `<delay>`). The query's iq *result* carries the `<fin>` + RSM bounds, which
//! we use to advance the stored cursor.

use async_channel::Sender;
use minidom::Element;

use mxc_store::Store;

use crate::client::{AccountConfig, Writer};
use crate::event::Event;
use crate::xeps::iq;
use crate::xeps::roster::new_id;

const NS_MAM: &str = "urn:xmpp:mam:2";
const NS_RSM: &str = "http://jabber.org/protocol/rsm";
const NS_DATA: &str = "jabber:x:data";

const PAGE: u32 = 50;

pub async fn load_page(
    w: &Writer,
    store: &Store,
    cfg: &AccountConfig,
    events: &Sender<Event>,
    conversation_id: i64,
    before: Option<String>,
) -> anyhow::Result<()> {
    let Some((jid, kind)) = store.conversation_target(conversation_id).await? else {
        return Ok(());
    };
    // MUC private messages aren't reliably archived per-occupant; re-querying the account MAM
    // with the full occupant JID just re-delivers (and can duplicate) live PMs. Skip MAM.
    if kind == "muc_pm" {
        return Ok(());
    }
    let is_muc = kind == "muc";

    // Cursor: explicit `before`, else the oldest id we already have (page backwards).
    let cursor = match before {
        Some(b) => Some(b),
        None => store.mam_cursor(cfg.account_id, &jid).await?.and_then(|c| c.first_id),
    };

    // Filter form: bind to this conversation (1:1 uses `with`; MUC queries the room MAM).
    let mut form = Element::builder("x", NS_DATA).attr(crate::ncname("type"), "submit").append(
        field("FORM_TYPE", NS_MAM, true),
    );
    if !is_muc {
        form = form.append(field("with", &jid, false));
    }

    // RSM: page backwards from the cursor, newest-of-page last.
    let mut set = Element::builder("set", NS_RSM)
        .append(Element::builder("max", NS_RSM).append(PAGE.to_string()).build());
    if let Some(c) = &cursor {
        set = set.append(Element::builder("before", NS_RSM).append(c.as_str()).build());
    } else {
        // empty <before/> = last page (most recent)
        set = set.append(Element::builder("before", NS_RSM).build());
    }

    let query = Element::builder("query", NS_MAM)
        .attr(crate::ncname("queryid"), new_id("mam"))
        .append(form.build())
        .append(set.build())
        .build();

    let mut req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "set")
        .attr(crate::ncname("id"), new_id("mam-iq"));
    if is_muc {
        req = req.attr(crate::ncname("to"), &jid); // query the MUC's archive
    }
    let req = req.append(query).build();

    let reply = iq::request(w, req).await?;

    // Parse <fin complete=..><set><first/><last/></set></fin>.
    if let Some(fin) = reply.get_child("fin", NS_MAM) {
        let complete = fin.attr("complete") == Some("true");
        let (first, last) = fin
            .get_child("set", NS_RSM)
            .map(|s| {
                (
                    s.get_child("first", NS_RSM).map(|e| e.text()),
                    s.get_child("last", NS_RSM).map(|e| e.text()),
                )
            })
            .unwrap_or((None, None));
        store
            .set_mam_cursor(cfg.account_id, &jid, first.as_deref(), last.as_deref(), complete)
            .await?;
    }

    // Tell the UI the conversation likely gained backfilled messages.
    if let Ok(items) = store.conversations(cfg.account_id).await {
        let _ = events.send(Event::ConversationsUpdated { account_id: cfg.account_id, items }).await;
    }
    Ok(())
}

/// Catch up on messages received since we last synced this conversation's archive: page
/// *forward* from the stored `last_id` until the server reports `complete`. The archived
/// messages arrive as separate stanzas (deduped on insert), so we only drive the paging here.
/// With no sync point yet, fall back to fetching the most recent page.
pub async fn catch_up(
    w: &Writer,
    store: &Store,
    cfg: &AccountConfig,
    events: &Sender<Event>,
    conversation_id: i64,
) -> anyhow::Result<()> {
    let Some((jid, kind)) = store.conversation_target(conversation_id).await? else {
        return Ok(());
    };
    // See `load_page`: don't drive MAM for MUC private messages.
    if kind == "muc_pm" {
        return Ok(());
    }
    let is_muc = kind == "muc";

    let Some(mut after) = store.mam_cursor(cfg.account_id, &jid).await?.and_then(|c| c.last_id)
    else {
        // Never synced → grab the most recent page (which records the cursor).
        return load_page(w, store, cfg, events, conversation_id, None).await;
    };

    // Page forward; bounded so a huge backlog can't loop forever in one go.
    for _ in 0..100 {
        let (complete, last) = query_after(w, &jid, is_muc, &after).await?;
        match last {
            Some(last_id) => {
                // Advance only the newest cursor (COALESCE keeps `first_id`).
                store
                    .set_mam_cursor(cfg.account_id, &jid, None, Some(&last_id), complete)
                    .await?;
                after = last_id;
            }
            None => break,
        }
        if complete {
            break;
        }
    }

    if let Ok(items) = store.conversations(cfg.account_id).await {
        let _ = events.send(Event::ConversationsUpdated { account_id: cfg.account_id, items }).await;
    }
    Ok(())
}

/// Cursor key for the *account* archive (1:1 + carbons), distinct from any per-conversation
/// cursor (which is keyed by contact JID). The leading control char is invalid in a JID, so it
/// can never collide with a real `with` archive — including the "Note to self" chat with our own
/// bare JID.
const ACCOUNT_ARCHIVE: &str = "\u{1}account";

/// Catch up the **account archive** (all 1:1 conversations + carbons) after a reconnect: page
/// forward from the last account-level archive id we synced. This is what fills the gap created
/// while the client was closed — including messages from contacts we had no local conversation
/// with yet (a forwarded archive message creates the conversation via `handle_incoming`). MUC
/// rooms have their own per-room archive and are caught up separately in `bootstrap`.
///
/// On the very first run (no cursor) we don't backfill the whole history — we just fetch the most
/// recent page (deduped on insert) and record its newest id as the baseline, so subsequent
/// restarts page forward from there.
pub async fn catch_up_account(
    w: &Writer,
    store: &Store,
    cfg: &AccountConfig,
    events: &Sender<Event>,
) -> anyhow::Result<()> {
    match store.mam_cursor(cfg.account_id, ACCOUNT_ARCHIVE).await?.and_then(|c| c.last_id) {
        // No baseline yet → fetch the most recent page and record where the archive currently
        // ends; we rely on live delivery for anything newer this session.
        None => {
            let page = query_account(w, None).await?;
            if let Some(last_id) = page.last {
                store
                    .set_mam_cursor(cfg.account_id, ACCOUNT_ARCHIVE, None, Some(&last_id), page.complete)
                    .await?;
            }
        }
        // Have a baseline → page forward until the server reports the archive is exhausted.
        Some(mut after) => {
            for _ in 0..200 {
                let page = query_account(w, Some(&after)).await?;
                match page.last {
                    Some(last_id) => {
                        store
                            .set_mam_cursor(
                                cfg.account_id,
                                ACCOUNT_ARCHIVE,
                                None,
                                Some(&last_id),
                                page.complete,
                            )
                            .await?;
                        after = last_id;
                    }
                    None => break,
                }
                if page.complete {
                    break;
                }
            }
        }
    }

    if let Ok(items) = store.conversations(cfg.account_id).await {
        let _ = events.send(Event::ConversationsUpdated { account_id: cfg.account_id, items }).await;
    }
    Ok(())
}

struct Page {
    complete: bool,
    last: Option<String>,
}

/// Run one account-archive MAM page: `<after>` the cursor, or — when `after` is `None` — the
/// most recent page (empty `<before/>`). No `with` filter and no `to`, so the query targets our
/// own account archive (1:1 + carbons across every contact).
async fn query_account(w: &Writer, after: Option<&str>) -> anyhow::Result<Page> {
    let form = Element::builder("x", NS_DATA)
        .attr(crate::ncname("type"), "submit")
        .append(field("FORM_TYPE", NS_MAM, true))
        .build();
    let mut set =
        Element::builder("set", NS_RSM).append(Element::builder("max", NS_RSM).append(PAGE.to_string()).build());
    set = match after {
        Some(a) => set.append(Element::builder("after", NS_RSM).append(a).build()),
        None => set.append(Element::builder("before", NS_RSM).build()), // empty <before/> = last page
    };
    let query = Element::builder("query", NS_MAM)
        .attr(crate::ncname("queryid"), new_id("mam"))
        .append(form)
        .append(set.build())
        .build();
    let req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "set")
        .attr(crate::ncname("id"), new_id("mam-iq"))
        .append(query)
        .build();
    let reply = iq::request(w, req).await?;

    if let Some(fin) = reply.get_child("fin", NS_MAM) {
        let complete = fin.attr("complete") == Some("true");
        let last = fin
            .get_child("set", NS_RSM)
            .and_then(|s| s.get_child("last", NS_RSM))
            .map(|e| e.text());
        Ok(Page { complete, last })
    } else {
        Ok(Page { complete: true, last: None })
    }
}

/// Run one forward MAM page (`<after>`), returning `(complete, last_id_of_page)`.
async fn query_after(
    w: &Writer,
    jid: &str,
    is_muc: bool,
    after: &str,
) -> anyhow::Result<(bool, Option<String>)> {
    let mut form = Element::builder("x", NS_DATA)
        .attr(crate::ncname("type"), "submit")
        .append(field("FORM_TYPE", NS_MAM, true));
    if !is_muc {
        form = form.append(field("with", jid, false));
    }
    let set = Element::builder("set", NS_RSM)
        .append(Element::builder("max", NS_RSM).append(PAGE.to_string()).build())
        .append(Element::builder("after", NS_RSM).append(after).build())
        .build();
    let query = Element::builder("query", NS_MAM)
        .attr(crate::ncname("queryid"), new_id("mam"))
        .append(form.build())
        .append(set)
        .build();
    let mut req =
        Element::builder("iq", "jabber:client").attr(crate::ncname("type"), "set").attr(crate::ncname("id"), new_id("mam-iq"));
    if is_muc {
        req = req.attr(crate::ncname("to"), jid);
    }
    let reply = iq::request(w, req.append(query).build()).await?;

    if let Some(fin) = reply.get_child("fin", NS_MAM) {
        let complete = fin.attr("complete") == Some("true");
        let last = fin
            .get_child("set", NS_RSM)
            .and_then(|s| s.get_child("last", NS_RSM))
            .map(|e| e.text());
        Ok((complete, last))
    } else {
        Ok((true, None))
    }
}

fn field(var: &str, value: &str, hidden: bool) -> Element {
    let mut f = Element::builder("field", NS_DATA).attr(crate::ncname("var"), var);
    if hidden {
        f = f.attr(crate::ncname("type"), "hidden");
    }
    f.append(Element::builder("value", NS_DATA).append(value).build()).build()
}
