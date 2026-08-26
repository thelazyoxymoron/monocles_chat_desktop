//! `FeedModel` — a `QAbstractListModel` of social-feed items (XEP-0472 microblog). Loaded in two
//! modes: `reload(jid)` shows the merged top-level posts (newest first, with a comment count) from
//! all followed feeds + our own; `loadComments(postId)` shows one post's replies (oldest first).
//! Items are accumulated in memory in `session` (not persisted).

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

const ROLE_ID: i32 = 256;
const ROLE_AUTHOR: i32 = 257;
const ROLE_TITLE: i32 = 258;
const ROLE_CONTENT: i32 = 259;
const ROLE_PUBLISHED: i32 = 260;
const ROLE_LINK: i32 = 261;
const ROLE_ATTACHMENT: i32 = 262;
const ROLE_OWN: i32 = 263;
const ROLE_COMMENTS: i32 = 264;
const ROLE_LIKES: i32 = 265;
const ROLE_LIKED: i32 = 266;

/// One feed item (post or comment), resolved for display.
#[derive(Clone, Default)]
pub struct FeedEntry {
    pub id: String,
    pub author: String,
    pub title: String,
    pub content: String,
    pub published: i64,
    pub link: String,
    pub attachment_url: String,
    pub own: bool,
    /// Number of replies (top-level posts only), excluding likes.
    pub comment_count: i64,
    /// Number of "♥" likes on the post, and whether we've liked it.
    pub like_count: i64,
    pub liked: bool,
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!(<QtCore/QAbstractListModel>);
        type QAbstractListModel;

        include!("cxx-qt-lib/qhash.h");
        type QHash_i32_QByteArray = cxx_qt_lib::QHash<cxx_qt_lib::QHashPair_i32_QByteArray>;

        include!("cxx-qt-lib/qvariant.h");
        type QVariant = cxx_qt_lib::QVariant;

        include!("cxx-qt-lib/qmodelindex.h");
        type QModelIndex = cxx_qt_lib::QModelIndex;

        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// Others' posts newer than the persisted "seen" cursor — the Feeds nav badge.
        /// Only `reload` (the top-level posts mode) updates it, not `loadComments`.
        #[qproperty(i32, unseen_count, cxx_name = "unseenCount")]
        type FeedModel = super::FeedModelRust;

        /// The user looked at the feed: remember the newest post as seen, badge → 0.
        #[qinvokable]
        #[cxx_name = "markSeen"]
        fn mark_seen(self: Pin<&mut FeedModel>);

        /// (Re)build the merged top-level feed (own posts flagged against `jid`).
        #[qinvokable]
        fn reload(self: Pin<&mut FeedModel>, jid: &QString);

        /// Show the replies (comments) of post `post_id`, oldest first.
        #[qinvokable]
        #[cxx_name = "loadComments"]
        fn load_comments(self: Pin<&mut FeedModel>, post_id: &QString);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &FeedModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(self: &FeedModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(self: &FeedModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut FeedModel>);
        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut FeedModel>);
    }
}

/// Backing data for the `FeedModel` QObject.
#[derive(Default)]
pub struct FeedModelRust {
    items: Vec<FeedEntry>,
    unseen_count: i32,
}

impl qobject::FeedModel {
    pub fn reload(mut self: Pin<&mut Self>, jid: &QString) {
        let items = crate::session::feed_posts(&jid.to_string());
        // Own posts don't count as "new" — the badge is about others' posts.
        let seen: i64 = crate::session::seen_mark("feeds").parse().unwrap_or(0);
        let unseen = items.iter().filter(|p| !p.own && p.published > seen).count() as i32;
        self.as_mut().reset(items);
        self.as_mut().set_unseen_count(unseen);
    }

    /// Persist the newest visible post as the seen cursor and clear the badge.
    pub fn mark_seen(mut self: Pin<&mut Self>) {
        if let Some(newest) = self.items.iter().map(|p| p.published).max() {
            crate::session::set_seen_mark("feeds", &newest.to_string());
        }
        self.as_mut().set_unseen_count(0);
    }

    pub fn load_comments(self: Pin<&mut Self>, post_id: &QString) {
        let items = crate::session::feed_comments(&post_id.to_string());
        self.reset(items);
    }

    pub fn reset(mut self: Pin<&mut Self>, items: Vec<FeedEntry>) {
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().items = items;
        self.as_mut().end_reset_model();
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_ID => QVariant::from(&QString::from(item.id.as_str())),
            ROLE_AUTHOR => QVariant::from(&QString::from(item.author.as_str())),
            ROLE_TITLE => QVariant::from(&QString::from(item.title.as_str())),
            ROLE_CONTENT => QVariant::from(&QString::from(item.content.as_str())),
            ROLE_PUBLISHED => QVariant::from(&item.published),
            ROLE_LINK => QVariant::from(&QString::from(item.link.as_str())),
            ROLE_ATTACHMENT => QVariant::from(&QString::from(item.attachment_url.as_str())),
            ROLE_OWN => QVariant::from(&item.own),
            ROLE_COMMENTS => QVariant::from(&item.comment_count),
            ROLE_LIKES => QVariant::from(&item.like_count),
            ROLE_LIKED => QVariant::from(&item.liked),
            _ => QVariant::default(),
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_ID, QByteArray::from("postId"));
        roles.insert(ROLE_AUTHOR, QByteArray::from("author"));
        roles.insert(ROLE_TITLE, QByteArray::from("title"));
        roles.insert(ROLE_CONTENT, QByteArray::from("content"));
        roles.insert(ROLE_PUBLISHED, QByteArray::from("published"));
        roles.insert(ROLE_LINK, QByteArray::from("link"));
        roles.insert(ROLE_ATTACHMENT, QByteArray::from("attachmentUrl"));
        roles.insert(ROLE_OWN, QByteArray::from("own"));
        roles.insert(ROLE_COMMENTS, QByteArray::from("commentCount"));
        roles.insert(ROLE_LIKES, QByteArray::from("likeCount"));
        roles.insert(ROLE_LIKED, QByteArray::from("liked"));
        roles
    }
}
