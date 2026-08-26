//! `StoryModel` — a `QAbstractListModel` of non-expired social-feed Stories (XEP PEP), for the
//! Stories section. `reload(jid)` queries the store on the core runtime; story media (images)
//! is fetched + cached lazily and the model reloaded when it lands.

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

const ROLE_UUID: i32 = 256;
const ROLE_CONTACT: i32 = 257;
const ROLE_TITLE: i32 = 258;
const ROLE_TYPE: i32 = 259;
const ROLE_PUBLISHED: i32 = 260;
const ROLE_OWN: i32 = 261;
const ROLE_LOCAL_PATH: i32 = 262;
const ROLE_AVATAR: i32 = 263;

/// One story row, resolved for display.
#[derive(Clone, Default)]
pub struct StoryEntry {
    pub uuid: String,
    pub contact: String,
    pub title: String,
    /// MIME type ("image/…" or "video/…").
    pub mime: String,
    /// Unix seconds.
    pub published: i64,
    /// Whether this is our own story (gets a delete button).
    pub own: bool,
    /// Cached media file path (empty until downloaded).
    pub local_path: String,
    /// Publisher's cached avatar path, or empty.
    pub avatar_path: String,
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
        /// Contacts' stories newer than the persisted "seen" cursor — the Stories nav badge.
        #[qproperty(i32, unseen_count, cxx_name = "unseenCount")]
        type StoryModel = super::StoryModelRust;

        /// The user looked at the feed: remember the newest story as seen, badge → 0.
        #[qinvokable]
        #[cxx_name = "markSeen"]
        fn mark_seen(self: Pin<&mut StoryModel>);

        /// (Re)load non-expired stories for the account.
        #[qinvokable]
        fn reload(self: Pin<&mut StoryModel>, jid: &QString);

        /// Re-read for the last-loaded JID (after a media download completes).
        #[qinvokable]
        #[cxx_name = "reloadSelf"]
        fn reload_self(self: Pin<&mut StoryModel>);

        /// MIME type / cached media path of the story at `index` (for the sequential viewer);
        /// empty if out of range / not yet downloaded.
        #[qinvokable]
        #[cxx_name = "mimeAt"]
        fn mime_at(self: &StoryModel, index: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "pathAt"]
        fn path_at(self: &StoryModel, index: i32) -> QString;
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &StoryModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(self: &StoryModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(self: &StoryModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut StoryModel>);
        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut StoryModel>);
    }

    impl cxx_qt::Threading for StoryModel {}
}

/// Backing data for the `StoryModel` QObject.
#[derive(Default)]
pub struct StoryModelRust {
    jid: String,
    items: Vec<StoryEntry>,
    unseen_count: i32,
}

impl qobject::StoryModel {
    pub fn reload(mut self: Pin<&mut Self>, jid: &QString) {
        self.as_mut().rust_mut().jid = jid.to_string();
        crate::session::load_stories(jid.to_string(), self.qt_thread());
    }

    pub fn reload_self(self: Pin<&mut Self>) {
        let jid = self.jid.clone();
        if !jid.is_empty() {
            crate::session::load_stories(jid, self.qt_thread());
        }
    }

    fn mime_at(&self, index: i32) -> QString {
        self.items
            .get(index as usize)
            .map(|s| QString::from(s.mime.as_str()))
            .unwrap_or_default()
    }

    fn path_at(&self, index: i32) -> QString {
        self.items
            .get(index as usize)
            .map(|s| QString::from(s.local_path.as_str()))
            .unwrap_or_default()
    }

    pub fn reset(mut self: Pin<&mut Self>, items: Vec<StoryEntry>) {
        // Own stories don't count as "new" — the badge is about contacts' posts.
        let seen: i64 = crate::session::seen_mark("stories").parse().unwrap_or(0);
        let unseen = items.iter().filter(|s| !s.own && s.published > seen).count() as i32;
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().items = items;
        self.as_mut().end_reset_model();
        self.as_mut().set_unseen_count(unseen);
    }

    /// Persist the newest visible story as the seen cursor and clear the badge.
    pub fn mark_seen(mut self: Pin<&mut Self>) {
        if let Some(newest) = self.items.iter().map(|s| s.published).max() {
            crate::session::set_seen_mark("stories", &newest.to_string());
        }
        self.as_mut().set_unseen_count(0);
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_UUID => QVariant::from(&QString::from(item.uuid.as_str())),
            ROLE_CONTACT => QVariant::from(&QString::from(item.contact.as_str())),
            ROLE_TITLE => QVariant::from(&QString::from(item.title.as_str())),
            ROLE_TYPE => QVariant::from(&QString::from(item.mime.as_str())),
            ROLE_PUBLISHED => QVariant::from(&item.published),
            ROLE_OWN => QVariant::from(&item.own),
            ROLE_LOCAL_PATH => QVariant::from(&QString::from(item.local_path.as_str())),
            ROLE_AVATAR => QVariant::from(&QString::from(item.avatar_path.as_str())),
            _ => QVariant::default(),
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_UUID, QByteArray::from("uuid"));
        roles.insert(ROLE_CONTACT, QByteArray::from("contact"));
        roles.insert(ROLE_TITLE, QByteArray::from("title"));
        roles.insert(ROLE_TYPE, QByteArray::from("mime"));
        roles.insert(ROLE_PUBLISHED, QByteArray::from("published"));
        roles.insert(ROLE_OWN, QByteArray::from("own"));
        roles.insert(ROLE_LOCAL_PATH, QByteArray::from("localPath"));
        roles.insert(ROLE_AVATAR, QByteArray::from("avatarPath"));
        roles
    }
}
