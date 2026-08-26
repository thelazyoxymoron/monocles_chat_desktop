//! Social feed — XEP-0472 / XEP-0277 microblogging, wire-compatible with monocles chat for
//! Android's "Feeds".
//!
//! Posts are Atom `<entry>` items in the author's `urn:xmpp:microblog:0` PEP node. Each post
//! links to a SEPARATE per-post comments node `urn:xmpp:microblog:0:comments/<post-id>` via
//! `<link rel="replies" href="xmpp:<author>?;node=…:comments/<id>"/>`; comments are Atom entries
//! in that node whose body is the `<title>` element (publish_model=open, so anyone can comment).
//! Items aren't cached in the store — the UI accumulates fetched lists in memory.

use minidom::Element;

use crate::client::{AccountConfig, Writer};
use crate::event::FeedPost;
use crate::xeps::pep;
use crate::xeps::{iq, roster::new_id};

pub const NS_MICROBLOG: &str = "urn:xmpp:microblog:0";
const NS_ATOM: &str = "http://www.w3.org/2005/Atom";
const NS_PUBSUB: &str = "http://jabber.org/protocol/pubsub";
const NS_XDATA: &str = "jabber:x:data";
const NS_NODE_CONFIG: &str = "http://jabber.org/protocol/pubsub#node_config";

fn comments_node(post_id: &str) -> String {
    format!("{NS_MICROBLOG}:comments/{post_id}")
}

// --- fetching ------------------------------------------------------------------------------

/// Fetch up to 100 of `jid`'s posts (None = our own); newest first. Best-effort.
pub async fn fetch(w: &Writer, jid: Option<&str>, owner_bare: &str) -> Vec<FeedPost> {
    let Ok(reply) = pep::items(w, jid, NS_MICROBLOG, Some(100)).await else {
        return Vec::new();
    };
    let mut posts: Vec<FeedPost> = pep::extract_items(&reply)
        .iter()
        .filter_map(|(id, entry)| parse_post(id.as_deref(), entry, owner_bare))
        .collect();
    posts.sort_by(|a, b| b.published.cmp(&a.published));
    posts
}

/// Fetch a post's comments from its `…:comments/<post_id>` node on `post_author`; oldest first.
pub async fn fetch_comments(w: &Writer, post_author: &str, post_id: &str) -> Vec<FeedPost> {
    let node = comments_node(post_id);
    let Ok(reply) = pep::items(w, Some(post_author), &node, Some(100)).await else {
        return Vec::new();
    };
    let mut comments: Vec<FeedPost> = pep::extract_items(&reply)
        .iter()
        .filter_map(|(id, entry)| parse_comment(id.as_deref(), entry))
        .collect();
    comments.sort_by(|a, b| a.published.cmp(&b.published));
    comments
}

// --- publishing ----------------------------------------------------------------------------

/// Publish a top-level post to our own feed, then ensure its comments node exists.
pub async fn publish_post(
    w: &Writer,
    cfg: &AccountConfig,
    title: &str,
    content: &str,
) -> anyhow::Result<()> {
    let post_id = uuid();
    // Ensure the feed node exists (best-effort; ignore "already exists").
    let _ = create_node(w, None, NS_MICROBLOG, post_config()).await;

    let comments_ref =
        format!("xmpp:{}?;node={}", cfg.bare(), comments_node(&post_id));
    let mut entry = Element::builder("entry", NS_ATOM)
        .append(Element::builder("id", NS_ATOM).append(format!("urn:uuid:{post_id}")).build())
        .append(
            Element::builder("link", NS_ATOM)
                .attr(crate::ncname("rel"), "replies")
                .attr(crate::ncname("title"), "comments")
                .attr(crate::ncname("href"), comments_ref)
                .build(),
        );
    if !title.is_empty() {
        entry = entry.append(
            Element::builder("title", NS_ATOM).attr(crate::ncname("type"), "text").append(title).build(),
        );
    }
    if !content.is_empty() {
        entry = entry.append(
            Element::builder("content", NS_ATOM).attr(crate::ncname("type"), "text").append(content).build(),
        );
    }
    let ts = crate::xeps::rfc3339_now();
    let entry = entry
        .append(author_element(cfg))
        .append(Element::builder("published", NS_ATOM).append(ts.as_str()).build())
        .append(Element::builder("updated", NS_ATOM).append(ts.as_str()).build())
        .build();

    pep::publish(w, NS_MICROBLOG, Some(&post_id), entry, None).await?;
    // Create the per-post comments node so contacts can reply (best-effort).
    let _ = create_node(w, None, &comments_node(&post_id), comments_config()).await;
    Ok(())
}

/// Publish a comment on `post_author`'s post `post_id` (its comments node; body in `<title>`).
pub async fn publish_comment(
    w: &Writer,
    cfg: &AccountConfig,
    post_author: &str,
    post_id: &str,
    content: &str,
) -> anyhow::Result<()> {
    let node = comments_node(post_id);
    // The comments node lives on the post author's service.
    let to = Some(post_author);
    // Ensure it exists (best-effort; the author normally created it with the post).
    let _ = create_node(w, to, &node, comments_config()).await;

    let ts = crate::xeps::rfc3339_now();
    let entry = Element::builder("entry", NS_ATOM)
        .append(Element::builder("title", NS_ATOM).append(content).build())
        .append(author_element(cfg))
        .append(Element::builder("id", NS_ATOM).append(format!("urn:uuid:{}", uuid())).build())
        .append(Element::builder("published", NS_ATOM).append(ts.as_str()).build())
        .append(Element::builder("updated", NS_ATOM).append(ts.as_str()).build())
        .build();
    let item = Element::builder("item", NS_PUBSUB).append(entry).build();
    let publish = Element::builder("publish", NS_PUBSUB).attr(crate::ncname("node"), node).append(item).build();
    let pubsub = Element::builder("pubsub", NS_PUBSUB).append(publish).build();
    let req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "set")
        .attr(crate::ncname("id"), new_id("pep-cmt"))
        .attr(crate::ncname("to"), post_author)
        .append(pubsub)
        .build();
    iq::request(w, req).await?;
    Ok(())
}

/// Retract a comment from a post's comments node (addressed to the post author's service).
pub async fn retract_comment(
    w: &Writer,
    post_author: &str,
    post_id: &str,
    comment_id: &str,
) -> anyhow::Result<()> {
    let node = comments_node(post_id);
    let retract = Element::builder("retract", NS_PUBSUB)
        .attr(crate::ncname("node"), node)
        .attr(crate::ncname("notify"), "true")
        .append(Element::builder("item", NS_PUBSUB).attr(crate::ncname("id"), comment_id).build())
        .build();
    let pubsub = Element::builder("pubsub", NS_PUBSUB).append(retract).build();
    let req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "set")
        .attr(crate::ncname("id"), new_id("pep-cmt-del"))
        .attr(crate::ncname("to"), post_author)
        .append(pubsub)
        .build();
    iq::request(w, req).await?;
    Ok(())
}

// --- helpers -------------------------------------------------------------------------------

fn uuid() -> String {
    new_id("post").trim_start_matches("post").trim_start_matches('-').to_string()
}

fn author_element(cfg: &AccountConfig) -> Element {
    Element::builder("author", NS_ATOM)
        .append(Element::builder("name", NS_ATOM).append(cfg.bare()).build())
        .append(Element::builder("uri", NS_ATOM).append(format!("xmpp:{}", cfg.bare())).build())
        .build()
}

/// XEP-0472 social-feed node config (presence access; only the owner publishes).
fn post_config() -> Element {
    node_config(&[
        ("pubsub#node_type", "leaf"),
        ("pubsub#access_model", "presence"),
        ("pubsub#persist_items", "1"),
        ("pubsub#max_items", "max"),
        ("pubsub#notify_retract", "1"),
        ("pubsub#publish_model", "publishers"),
    ])
}

/// Per-post comments node config (open access + open publish, so anyone can comment).
fn comments_config() -> Element {
    node_config(&[
        ("pubsub#node_type", "leaf"),
        ("pubsub#access_model", "open"),
        ("pubsub#persist_items", "1"),
        ("pubsub#max_items", "max"),
        ("pubsub#notify_retract", "1"),
        ("pubsub#publish_model", "open"),
    ])
}

fn node_config(fields: &[(&str, &str)]) -> Element {
    let mut x = Element::builder("x", NS_XDATA).attr(crate::ncname("type"), "submit").append(
        Element::builder("field", NS_XDATA)
            .attr(crate::ncname("var"), "FORM_TYPE")
            .attr(crate::ncname("type"), "hidden")
            .append(Element::builder("value", NS_XDATA).append(NS_NODE_CONFIG).build())
            .build(),
    );
    for (var, value) in fields {
        x = x.append(
            Element::builder("field", NS_XDATA)
                .attr(crate::ncname("var"), *var)
                .append(Element::builder("value", NS_XDATA).append(*value).build())
                .build(),
        );
    }
    x.build()
}

/// Create + configure a PubSub node (best-effort; conflict/forbidden are ignored by the caller).
async fn create_node(
    w: &Writer,
    to: Option<&str>,
    node: &str,
    config: Element,
) -> anyhow::Result<()> {
    let create = Element::builder("create", NS_PUBSUB).attr(crate::ncname("node"), node).build();
    let configure = Element::builder("configure", NS_PUBSUB).append(config).build();
    let pubsub = Element::builder("pubsub", NS_PUBSUB).append(create).append(configure).build();
    let mut req = Element::builder("iq", "jabber:client")
        .attr(crate::ncname("type"), "set")
        .attr(crate::ncname("id"), new_id("pep-create"));
    if let Some(j) = to {
        req = req.attr(crate::ncname("to"), j);
    }
    iq::request(w, req.append(pubsub).build()).await?;
    Ok(())
}

// --- parsing -------------------------------------------------------------------------------

fn author_of(entry: &Element, fallback: &str) -> String {
    entry
        .get_child("author", NS_ATOM)
        .and_then(|a| a.get_child("uri", NS_ATOM))
        .map(|u| u.text())
        .and_then(|uri| {
            uri.strip_prefix("xmpp:")
                .map(|s| s.split(['/', '?']).next().unwrap_or(s).to_string())
        })
        .unwrap_or_else(|| fallback.to_string())
}

fn published_of(entry: &Element) -> i64 {
    entry
        .get_child("published", NS_ATOM)
        .or_else(|| entry.get_child("updated", NS_ATOM))
        .map(|e| e.text())
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t.trim()).ok())
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp())
}

fn item_id_of(item_id: Option<&str>, entry: &Element) -> String {
    item_id
        .map(str::to_string)
        .or_else(|| {
            entry.get_child("id", NS_ATOM).map(|e| {
                let t = e.text();
                t.trim().strip_prefix("urn:uuid:").unwrap_or(t.trim()).to_string()
            })
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(uuid)
}

/// Parse one Atom `<entry>` from the microblog node into a top-level [`FeedPost`].
fn parse_post(item_id: Option<&str>, entry: &Element, owner_bare: &str) -> Option<FeedPost> {
    if entry.name() != "entry" {
        return None;
    }
    let title = entry.get_child("title", NS_ATOM).map(|t| t.text()).unwrap_or_default();
    let content = entry.get_child("content", NS_ATOM).map(|t| t.text()).unwrap_or_default();

    let (mut link, mut attachment_url, mut attachment_type) =
        (String::new(), String::new(), String::new());
    for l in entry.children().filter(|c| c.name() == "link") {
        let href = l.attr("href").unwrap_or_default().to_string();
        if href.is_empty() {
            continue;
        }
        match l.attr("rel") {
            Some("enclosure") => {
                attachment_url = href;
                attachment_type = l.attr("type").unwrap_or("application/octet-stream").to_string();
            }
            Some("related") | Some("alternate") => {
                if link.is_empty() {
                    link = href;
                }
            }
            _ => {}
        }
    }

    if title.trim().is_empty() && content.trim().is_empty() && attachment_url.is_empty() {
        return None;
    }
    Some(FeedPost {
        id: item_id_of(item_id, entry),
        author: author_of(entry, owner_bare),
        title,
        content,
        published: published_of(entry),
        link,
        attachment_url,
        attachment_type,
    })
}

/// Parse one comment entry (its body is the `<title>`).
fn parse_comment(item_id: Option<&str>, entry: &Element) -> Option<FeedPost> {
    if entry.name() != "entry" {
        return None;
    }
    // Comment body lives in <title>; fall back to <content> for tolerance.
    let mut content = entry.get_child("title", NS_ATOM).map(|t| t.text()).unwrap_or_default();
    if content.trim().is_empty() {
        content = entry.get_child("content", NS_ATOM).map(|t| t.text()).unwrap_or_default();
    }
    if content.trim().is_empty() {
        return None;
    }
    Some(FeedPost {
        id: item_id_of(item_id, entry),
        author: author_of(entry, ""),
        title: String::new(),
        content,
        published: published_of(entry),
        link: String::new(),
        attachment_url: String::new(),
        attachment_type: String::new(),
    })
}
