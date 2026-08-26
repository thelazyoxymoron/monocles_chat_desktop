//! `RosterModel` — a `QAbstractListModel` of the account's contacts (XEP roster), used by
//! the "new chat" contacts page. `reload(jid)` queries the store on the core runtime.

use std::pin::Pin;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

use mxc_store::RosterItem as StoreRosterItem;

const ROLE_JID: i32 = 256;
const ROLE_NAME: i32 = 257;
const ROLE_PRESENCE: i32 = 258;
const ROLE_AVATAR: i32 = 259;

/// One contact row.
#[derive(Clone, Default)]
pub struct RosterEntry {
    pub jid: String,
    pub name: String,
    /// "online"/"away"/"xa"/"dnd"/"offline". Set in session.rs.
    pub presence: String,
    /// Cached avatar file path, or empty. Set in session.rs.
    pub avatar_path: String,
}

impl RosterEntry {
    pub fn from_item(item: &StoreRosterItem) -> Self {
        let name = match &item.name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => item.jid.split('@').next().unwrap_or(&item.jid).to_string(),
        };
        Self {
            jid: item.jid.clone(),
            name,
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
        #[qobject]
        #[base = QAbstractListModel]
        #[qml_element]
        type RosterModel = super::RosterModelRust;

        /// (Re)load the account's contacts.
        #[qinvokable]
        fn reload(self: Pin<&mut RosterModel>, jid: &QString);

        /// Set the case-insensitive substring filter (matched against name and JID);
        /// an empty string shows every contact.
        #[qinvokable]
        #[rust_name = "set_filter"]
        fn setFilter(self: Pin<&mut RosterModel>, query: &QString);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &RosterModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(self: &RosterModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(self: &RosterModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut RosterModel>);
        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut RosterModel>);
        // For in-place row updates (same contacts) without losing the scroll position.
        #[inherit]
        fn index(self: &RosterModel, row: i32, column: i32, parent: &QModelIndex) -> QModelIndex;
        #[inherit]
        #[rust_name = "data_changed"]
        fn dataChanged(
            self: Pin<&mut RosterModel>,
            top_left: &QModelIndex,
            bottom_right: &QModelIndex,
            roles: &QList_i32,
        );
    }

    impl cxx_qt::Threading for RosterModel {}
}

/// Backing data for the `RosterModel` QObject.
#[derive(Default)]
pub struct RosterModelRust {
    /// The full roster as loaded from the store.
    all_items: Vec<RosterEntry>,
    /// The rows actually shown — `all_items` narrowed by `filter`.
    items: Vec<RosterEntry>,
    /// Case-insensitive substring filter on name/JID; empty = show all.
    filter: String,
}

impl qobject::RosterModel {
    pub fn reload(self: Pin<&mut Self>, jid: &QString) {
        crate::session::load_roster(jid.to_string(), self.qt_thread());
    }

    pub fn reset(mut self: Pin<&mut Self>, items: Vec<RosterEntry>) {
        self.as_mut().rust_mut().all_items = items;
        self.apply_filter();
    }

    pub fn set_filter(mut self: Pin<&mut Self>, query: &QString) {
        let next = query.to_string().trim().to_lowercase();
        if self.filter == next {
            return;
        }
        self.as_mut().rust_mut().filter = next;
        self.apply_filter();
    }

    /// Recompute the visible rows from `all_items` + `filter`. Same contacts in the same
    /// order (presence/avatar refresh) → update in place; otherwise a full reset. Either
    /// way this avoids throwing away the list's scroll position on every refresh.
    fn apply_filter(mut self: Pin<&mut Self>) {
        let filter = self.filter.clone();
        let next: Vec<RosterEntry> = if filter.is_empty() {
            self.all_items.clone()
        } else {
            self.all_items
                .iter()
                .filter(|e| {
                    e.name.to_lowercase().contains(&filter) || e.jid.to_lowercase().contains(&filter)
                })
                .cloned()
                .collect()
        };
        let same = self.items.len() == next.len()
            && self.items.iter().zip(&next).all(|(a, b)| a.jid == b.jid);
        if same {
            let count = next.len() as i32;
            self.as_mut().rust_mut().items = next;
            if count > 0 {
                let parent = QModelIndex::default();
                let top = self.as_ref().index(0, 0, &parent);
                let bottom = self.as_ref().index(count - 1, 0, &parent);
                self.as_mut().data_changed(&top, &bottom, &cxx_qt_lib::QList::<i32>::default());
            }
            return;
        }
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().items = next;
        self.as_mut().end_reset_model();
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_JID => QVariant::from(&QString::from(item.jid.as_str())),
            ROLE_NAME => QVariant::from(&QString::from(item.name.as_str())),
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
        roles.insert(ROLE_JID, QByteArray::from("jid"));
        roles.insert(ROLE_NAME, QByteArray::from("name"));
        roles.insert(ROLE_PRESENCE, QByteArray::from("presence"));
        roles.insert(ROLE_AVATAR, QByteArray::from("avatarPath"));
        roles
    }
}
