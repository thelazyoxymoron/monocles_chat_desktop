//! IQ request/response correlation.
//!
//! tokio-xmpp surfaces the stream as a flat sequence of stanzas, so a `get`/`set` that
//! expects a typed reply needs us to match the response `id` back to the awaiting task.
//! This is a tiny process-global registry of pending iq ids → oneshot senders (there is
//! effectively one connection per runtime). [`request`] sends an iq and awaits its
//! result; the router calls [`try_resolve`] on every inbound iq to fulfil waiters.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use minidom::Element;
use tokio::sync::oneshot;

static PENDING: OnceLock<Mutex<HashMap<String, oneshot::Sender<Element>>>> = OnceLock::new();

fn pending() -> &'static Mutex<HashMap<String, oneshot::Sender<Element>>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register(id: &str) -> oneshot::Receiver<Element> {
    let (tx, rx) = oneshot::channel();
    pending().lock().unwrap().insert(id.to_string(), tx);
    rx
}

/// If `iq` is a result/error reply we're awaiting, deliver it and return `true`.
pub fn try_resolve(iq: &Element) -> bool {
    if iq.name() != "iq" {
        return false;
    }
    match iq.attr("type") {
        Some("result") | Some("error") => {}
        _ => return false,
    }
    let Some(id) = iq.attr("id") else { return false };
    if let Some(tx) = pending().lock().unwrap().remove(id) {
        let _ = tx.send(iq.clone());
        true
    } else {
        false
    }
}

/// Send an iq (which MUST carry an `id`) and await its reply (30s timeout).
/// Returns `Err` if the reply is `type='error'`.
///
/// Safe to call only from a task *other* than the reader loop (e.g. spawned command or
/// bootstrap tasks), since it awaits a reply the reader loop must deliver.
pub async fn request(w: &crate::client::Writer, iq: Element) -> anyhow::Result<Element> {
    let id = iq
        .attr("id")
        .ok_or_else(|| anyhow::anyhow!("iq request missing id"))?
        .to_string();
    let rx = register(&id);
    w.send(iq)?;
    let reply = tokio::time::timeout(Duration::from_secs(30), rx)
        .await
        .map_err(|_| anyhow::anyhow!("iq {id} timed out"))??;
    if reply.attr("type") == Some("error") {
        let condition = reply
            .get_child("error", "jabber:client")
            .and_then(|e| e.children().next().map(|c| c.name().to_string()))
            .unwrap_or_else(|| "unknown".into());
        anyhow::bail!("iq {id} error: {condition}");
    }
    Ok(reply)
}
