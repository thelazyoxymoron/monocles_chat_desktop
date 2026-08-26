//! `MessageSearchModel` — a `QAbstractListModel` of message-search hits across all of the
//! account's conversations, backing the chats-list search box. `search(jid, query)` runs the
//! substring query on the core runtime; clicking a row opens that conversation and jumps to
//! the message (see `MessageModel::openAround` + the QML `jumpReady` handler).

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

use mxc_store::MessageSearchRow;

const ROLE_CONV_ID: i32 = 256;
const ROLE_MESSAGE_ID: i32 = 257;
const ROLE_JID: i32 = 258;
const ROLE_NAME: i32 = 259;
const ROLE_KIND: i32 = 260;
const ROLE_ENCRYPTED: i32 = 261;
const ROLE_MARKER: i32 = 262;
const ROLE_SNIPPET: i32 = 263;
const ROLE_TIMESTAMP: i32 = 264;
const ROLE_AVATAR: i32 = 265;
const ROLE_OUTGOING: i32 = 266;
const ROLE_SENDER: i32 = 267;

/// One search hit, resolved for display.
#[derive(Clone, Default)]
pub struct SearchResult {
    pub conversation_id: i64,
    pub message_id: i64,
    pub jid: String,
    pub name: String,
    pub kind: String,
    pub encrypted: bool,
    /// The id used to jump to the message (origin id, else stanza id).
    pub marker: String,
    /// The matched message body, normalised for display (media → a short label).
    pub snippet: String,
    pub timestamp: String,
    /// Cached avatar file path of the conversation, or empty. Set in session.rs.
    pub avatar_path: String,
    pub outgoing: bool,
    /// Sender nick (MUC messages) — shown as a prefix on the snippet.
    pub sender: String,
}

impl SearchResult {
    pub fn from_row(row: &MessageSearchRow) -> Self {
        // Conversation display name — mirrors ConversationItem::from_conv.
        let name = if row.conv_kind == "chat" && crate::session::is_own_bare(&row.conv_jid) {
            crate::session::NOTE_TO_SELF.to_string()
        } else {
            match &row.conv_name {
                Some(n) if !n.is_empty() => n.clone(),
                _ => match row.conv_kind.as_str() {
                    "muc_pm" => row.conv_jid.rsplit('/').next().unwrap_or(&row.conv_jid).to_string(),
                    _ => row.conv_jid.split('@').next().unwrap_or(&row.conv_jid).to_string(),
                },
            }
        };
        let raw = row.body.clone().unwrap_or_default();
        let snippet = if crate::messages::image_url(&raw).is_some() {
            "🖼 Image".to_string()
        } else if crate::messages::audio_url(&raw).is_some() {
            "🎤 Voice message".to_string()
        } else if raw.trim().starts_with("cid:") && !raw.trim().contains(char::is_whitespace) {
            "🙂 Sticker".to_string()
        } else {
            raw
        };
        Self {
            conversation_id: row.conversation_id,
            message_id: row.message_id,
            jid: row.conv_jid.clone(),
            name,
            kind: row.conv_kind.clone(),
            encrypted: row.conv_encryption == "omemo2",
            marker: row.marker.clone().unwrap_or_default(),
            snippet,
            timestamp: row.timestamp.clone(),
            avatar_path: String::new(),
            outgoing: row.direction == "out",
            sender: match row.counterpart.rsplit_once('/') {
                Some((_, nick)) => nick.to_string(),
                None => String::new(),
            },
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
    }

    extern "RustQt" {
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        type MessageSearchModel = super::MessageSearchModelRust;

        /// Run a substring search over `account_jid`'s message history. With `scope_jid`
        /// empty the search spans all conversations; set to a conversation JID it's scoped
        /// to that chat. An empty/blank query clears the results.
        #[qinvokable]
        fn search(
            self: Pin<&mut MessageSearchModel>,
            account_jid: &QString,
            scope_jid: &QString,
            query: &QString,
        );

        /// Drop all results.
        #[qinvokable]
        fn clear(self: Pin<&mut MessageSearchModel>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &MessageSearchModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(self: &MessageSearchModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(self: &MessageSearchModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut MessageSearchModel>);
        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut MessageSearchModel>);
    }

    impl cxx_qt::Threading for MessageSearchModel {}
}

/// Backing data for the `MessageSearchModel` QObject.
#[derive(Default)]
pub struct MessageSearchModelRust {
    items: Vec<SearchResult>,
}

impl qobject::MessageSearchModel {
    pub fn search(self: Pin<&mut Self>, account_jid: &QString, scope_jid: &QString, query: &QString) {
        crate::session::search_messages(
            account_jid.to_string(),
            scope_jid.to_string(),
            query.to_string(),
            self.qt_thread(),
        );
    }

    pub fn clear(self: Pin<&mut Self>) {
        self.reset(Vec::new());
    }

    /// Replace all rows (called from the core runtime via the Qt thread queue).
    pub fn reset(mut self: Pin<&mut Self>, items: Vec<SearchResult>) {
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().items = items;
        self.as_mut().end_reset_model();
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_CONV_ID => QVariant::from(&item.conversation_id),
            ROLE_MESSAGE_ID => QVariant::from(&item.message_id),
            ROLE_JID => QVariant::from(&QString::from(item.jid.as_str())),
            ROLE_NAME => QVariant::from(&QString::from(item.name.as_str())),
            ROLE_KIND => QVariant::from(&QString::from(item.kind.as_str())),
            ROLE_ENCRYPTED => QVariant::from(&item.encrypted),
            ROLE_MARKER => QVariant::from(&QString::from(item.marker.as_str())),
            ROLE_SNIPPET => QVariant::from(&QString::from(item.snippet.as_str())),
            ROLE_TIMESTAMP => QVariant::from(&QString::from(item.timestamp.as_str())),
            ROLE_AVATAR => QVariant::from(&QString::from(item.avatar_path.as_str())),
            ROLE_OUTGOING => QVariant::from(&item.outgoing),
            ROLE_SENDER => QVariant::from(&QString::from(item.sender.as_str())),
            _ => QVariant::default(),
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_CONV_ID, QByteArray::from("convId"));
        roles.insert(ROLE_MESSAGE_ID, QByteArray::from("messageId"));
        roles.insert(ROLE_JID, QByteArray::from("jid"));
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_KIND, QByteArray::from("kind"));
        roles.insert(ROLE_ENCRYPTED, QByteArray::from("encrypted"));
        roles.insert(ROLE_MARKER, QByteArray::from("marker"));
        roles.insert(ROLE_SNIPPET, QByteArray::from("snippet"));
        roles.insert(ROLE_TIMESTAMP, QByteArray::from("timestamp"));
        roles.insert(ROLE_AVATAR, QByteArray::from("avatarPath"));
        roles.insert(ROLE_OUTGOING, QByteArray::from("outgoing"));
        roles.insert(ROLE_SENDER, QByteArray::from("sender"));
        roles
    }
}
