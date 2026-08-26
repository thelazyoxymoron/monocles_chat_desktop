//! `ConversationModel` — a `QAbstractListModel` backing the chat list in QML.
//!
//! The Rust side holds a `Vec<ConversationItem>`; QML's `ListView` reads rows by role
//! name (see `role_names`). `reload(jid)` kicks an async DB query on the core runtime
//! (in [`crate::session`]) which queues `reset(items)` back onto the Qt thread.

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

use mxc_store::Conversation;

// Custom roles start at Qt::UserRole (256).
const ROLE_ID: i32 = 256;
const ROLE_JID: i32 = 257;
const ROLE_NAME: i32 = 258;
const ROLE_KIND: i32 = 259;
const ROLE_UNREAD: i32 = 260;
const ROLE_ENCRYPTED: i32 = 261;
const ROLE_LAST_ACTIVE: i32 = 262;
const ROLE_PRESENCE: i32 = 263;
const ROLE_AVATAR: i32 = 264;

/// One conversation row, already resolved for display.
#[derive(Clone, Default)]
pub struct ConversationItem {
    pub id: i64,
    pub jid: String,
    pub kind: String,
    pub name: String,
    pub encrypted: bool,
    pub unread: i64,
    pub last_active: String,
    /// "online"/"away"/"xa"/"dnd"/"offline" (1:1 chats); empty for MUCs. Set in session.rs.
    pub presence: String,
    /// Cached avatar file path, or empty. Set in session.rs.
    pub avatar_path: String,
}

impl ConversationItem {
    /// Map a stored conversation to a display row (mirrors the GTK list's name logic).
    pub fn from_conv(c: &Conversation) -> Self {
        let name = if c.kind == "chat" && crate::session::is_own_bare(&c.jid) {
            // A 1:1 chat with our own account is shown as "Note to self".
            crate::session::NOTE_TO_SELF.to_string()
        } else {
            match &c.name {
                Some(n) if !n.is_empty() => n.clone(),
                _ => match c.kind.as_str() {
                    // muc_pm jid is `room@host/nick` → show the nick.
                    "muc_pm" => c.jid.rsplit('/').next().unwrap_or(&c.jid).to_string(),
                    // otherwise the local part of the bare jid.
                    _ => c.jid.split('@').next().unwrap_or(&c.jid).to_string(),
                },
            }
        };
        Self {
            id: c.id,
            jid: c.jid.clone(),
            kind: c.kind.clone(),
            name,
            encrypted: c.encryption == "omemo2",
            unread: c.unread,
            last_active: c.last_active.clone().unwrap_or_default(),
            presence: String::new(),
            avatar_path: String::new(),
        }
    }
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

        include!("cxx-qt-lib/qlist.h");
        type QList_i32 = cxx_qt_lib::QList<i32>;
    }

    extern "RustQt" {
        /// Exposed to QML as `ConversationModel` under the `de.monocles.chat` import.
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        /// Sum of all conversations' unread counters — the Chats nav badge.
        #[qproperty(i64, unread_total, cxx_name = "unreadTotal")]
        type ConversationModel = super::ConversationModelRust;

        /// (Re)load the account's conversations from the local store.
        #[qinvokable]
        fn reload(self: Pin<&mut ConversationModel>, jid: &QString);
    }

    // QAbstractItemModel overrides. cxx-qt keeps the declared name as the C++ method name
    // (no auto camelCasing), so we must use the exact C++ virtual names and map them to
    // snake_case Rust via #[rust_name].
    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &ConversationModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(self: &ConversationModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(self: &ConversationModel) -> QHash_i32_QByteArray;
    }

    // Inherited protected helpers used to mutate the model from Rust (camelCase C++ names).
    unsafe extern "RustQt" {
        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut ConversationModel>);
        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut ConversationModel>);
        // For in-place row updates (same conversations) without losing the scroll position.
        #[inherit]
        fn index(self: &ConversationModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[rust_name = "data_changed"]
        fn dataChanged(
            self: Pin<&mut ConversationModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }

    impl cxx_qt::Threading for ConversationModel {}
}

/// Backing data for the `ConversationModel` QObject.
#[derive(Default)]
pub struct ConversationModelRust {
    items: Vec<ConversationItem>,
    unread_total: i64,
}

impl qobject::ConversationModel {
    /// QML entry point: load this account's conversations (async, off the Qt thread).
    pub fn reload(self: Pin<&mut Self>, jid: &QString) {
        crate::session::load_conversations(jid.to_string(), self.qt_thread());
    }

    /// Replace all rows (called from the core runtime via the Qt thread queue). The same
    /// conversations in the same order (presence/avatar/unread changes) update in place —
    /// a full reset would throw away the list's scroll position on every refresh. A new
    /// message reorders by last_active, which is a genuine reset.
    pub fn reset(mut self: Pin<&mut Self>, items: Vec<ConversationItem>) {
        let total: i64 = items.iter().map(|i| i.unread).sum();
        let same = self.items.len() == items.len()
            && self.items.iter().zip(&items).all(|(a, b)| a.id == b.id);
        if same {
            let count = items.len() as i32;
            self.as_mut().rust_mut().items = items;
            if count > 0 {
                let parent = QModelIndex::default();
                let top = self.as_ref().index(0, 0, &parent);
                let bottom = self.as_ref().index(count - 1, 0, &parent);
                self.as_mut().data_changed(&top, &bottom, &cxx_qt_lib::QList::<i32>::default());
            }
        } else {
            self.as_mut().begin_reset_model();
            self.as_mut().rust_mut().items = items;
            self.as_mut().end_reset_model();
        }
        self.as_mut().set_unread_total(total);
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_ID => QVariant::from(&item.id),
            ROLE_JID => QVariant::from(&QString::from(item.jid.as_str())),
            ROLE_NAME => QVariant::from(&QString::from(item.name.as_str())),
            ROLE_KIND => QVariant::from(&QString::from(item.kind.as_str())),
            ROLE_UNREAD => QVariant::from(&item.unread),
            ROLE_ENCRYPTED => QVariant::from(&item.encrypted),
            ROLE_LAST_ACTIVE => QVariant::from(&QString::from(item.last_active.as_str())),
            ROLE_PRESENCE => QVariant::from(&QString::from(item.presence.as_str())),
            ROLE_AVATAR => QVariant::from(&QString::from(item.avatar_path.as_str())),
            _ => QVariant::default(),
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_ID, QByteArray::from("convId"));
        roles.insert(ROLE_JID, QByteArray::from("jid"));
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_KIND, QByteArray::from("kind"));
        roles.insert(ROLE_UNREAD, QByteArray::from("unread"));
        roles.insert(ROLE_ENCRYPTED, QByteArray::from("encrypted"));
        roles.insert(ROLE_LAST_ACTIVE, QByteArray::from("lastActive"));
        roles.insert(ROLE_PRESENCE, QByteArray::from("presence"));
        roles.insert(ROLE_AVATAR, QByteArray::from("avatarPath"));
        roles
    }
}
