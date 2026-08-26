//! `DeviceModel` — a `QAbstractListModel` of OMEMO2 device keys (a contact's, or our own),
//! for the trust / fingerprint-verification UI.
//!
//! Like `OccupantModel`, the rows come from a synchronously-readable global cache that the
//! event pump fills from `Event::ContactKeys` / `Event::OwnKeys` (see `crate::session`).
//! `load(jid)` / `loadOwn()` request a fresh copy from the core and reset from the cache;
//! when the reply lands, the `Backend::keysChanged` signal tells QML to call `reload()`.

use std::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QByteArray, QHash, QHashPair_i32_QByteArray, QModelIndex, QString, QVariant};

const ROLE_DEVICE_ID: i32 = 256;
const ROLE_FINGERPRINT: i32 = 257;
const ROLE_TRUST: i32 = 258;
const ROLE_ACTIVE: i32 = 259;
const ROLE_IS_OWN: i32 = 260;

/// The cache key used for our own devices (an impossible JID, so it never collides with a
/// contact's bare JID).
pub const OWN_KEY: &str = "__own__";

/// One OMEMO2 device, mirroring `mxc_proto::DeviceKey` plus an `is_own`/`is_this` marker.
#[derive(Clone, Default)]
pub struct DeviceEntry {
    pub device_id: i64,
    /// Space-grouped hex of the identity key (already formatted by the core).
    pub fingerprint: String,
    /// 0 = undecided, 1 = trusted/enabled, 2 = untrusted/disabled.
    pub trust: i64,
    /// Whether the device is still in the published device list.
    pub active: bool,
    /// True for *this* device (own fingerprint) — shown but not toggleable.
    pub is_this: bool,
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
        type DeviceModel = super::DeviceModelRust;

        /// (Re)load a contact's devices (bare JID): request a fresh copy and show the cache.
        #[qinvokable]
        fn load(self: Pin<&mut DeviceModel>, jid: &QString);

        /// (Re)load our own devices (this device + our other devices).
        #[qinvokable]
        #[cxx_name = "loadOwn"]
        fn load_own(self: Pin<&mut DeviceModel>);

        /// Re-read the cache for the currently-shown JID (called from `Backend::keysChanged`).
        #[qinvokable]
        fn reload(self: Pin<&mut DeviceModel>);
    }

    extern "RustQt" {
        #[qinvokable]
        #[cxx_override]
        fn data(self: &DeviceModel, index: &QModelIndex, role: i32) -> QVariant;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "row_count"]
        fn rowCount(self: &DeviceModel, parent: &QModelIndex) -> i32;

        #[qinvokable]
        #[cxx_override]
        #[rust_name = "role_names"]
        fn roleNames(self: &DeviceModel) -> QHash_i32_QByteArray;
    }

    unsafe extern "RustQt" {
        #[inherit]
        #[rust_name = "begin_reset_model"]
        fn beginResetModel(self: Pin<&mut DeviceModel>);
        #[inherit]
        #[rust_name = "end_reset_model"]
        fn endResetModel(self: Pin<&mut DeviceModel>);
    }
}

/// Backing data for the `DeviceModel` QObject.
#[derive(Default)]
pub struct DeviceModelRust {
    /// The JID currently shown (a contact's bare JID, or [`OWN_KEY`]).
    jid: String,
    items: Vec<DeviceEntry>,
}

impl qobject::DeviceModel {
    pub fn load(mut self: Pin<&mut Self>, jid: &QString) {
        let jid = jid.to_string();
        crate::session::request_contact_keys(&jid);
        self.as_mut().rust_mut().jid = jid;
        self.reset_from_cache();
    }

    pub fn load_own(mut self: Pin<&mut Self>) {
        crate::session::request_own_keys();
        self.as_mut().rust_mut().jid = OWN_KEY.to_string();
        self.reset_from_cache();
    }

    pub fn reload(self: Pin<&mut Self>) {
        self.reset_from_cache();
    }

    /// Replace the rows from the global cache for the current JID.
    fn reset_from_cache(mut self: Pin<&mut Self>) {
        let jid = self.jid.clone();
        let items = crate::session::devices_for(&jid);
        self.as_mut().begin_reset_model();
        self.as_mut().rust_mut().items = items;
        self.as_mut().end_reset_model();
    }

    fn data(&self, index: &QModelIndex, role: i32) -> QVariant {
        let Some(item) = self.items.get(index.row() as usize) else {
            return QVariant::default();
        };
        match role {
            ROLE_DEVICE_ID => QVariant::from(&item.device_id),
            ROLE_FINGERPRINT => QVariant::from(&QString::from(item.fingerprint.as_str())),
            ROLE_TRUST => QVariant::from(&item.trust),
            ROLE_ACTIVE => QVariant::from(&item.active),
            ROLE_IS_OWN => QVariant::from(&item.is_this),
            _ => QVariant::default(),
        }
    }

    fn row_count(&self, _parent: &QModelIndex) -> i32 {
        self.items.len() as i32
    }

    fn role_names(&self) -> QHash<QHashPair_i32_QByteArray> {
        let mut roles = QHash::<QHashPair_i32_QByteArray>::default();
        roles.insert(ROLE_DEVICE_ID, QByteArray::from("deviceId"));
        roles.insert(ROLE_FINGERPRINT, QByteArray::from("fingerprint"));
        roles.insert(ROLE_TRUST, QByteArray::from("trust"));
        roles.insert(ROLE_ACTIVE, QByteArray::from("active"));
        roles.insert(ROLE_IS_OWN, QByteArray::from("isOwn"));
        roles
    }
}
