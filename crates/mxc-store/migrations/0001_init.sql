-- monocles chat desktop — initial schema
-- Mirrors the relevant parts of the Android app's DatabaseBackend so semantics line up
-- (accounts, roster, conversations, messages, MAM cursors) and adds the PQ OMEMO2 stores.

PRAGMA foreign_keys = ON;

-- ---------------------------------------------------------------------------
-- Accounts
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS accounts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    jid             TEXT    NOT NULL UNIQUE,          -- bare JID, e.g. arne@monocles.eu
    -- password is NEVER stored here; it lives in the secret service (libsecret/oo7),
    -- keyed by this account id. This column only records whether a secret exists.
    has_secret      INTEGER NOT NULL DEFAULT 0,
    resource        TEXT,                             -- last bound resource
    enabled         INTEGER NOT NULL DEFAULT 1,
    -- OMEMO2 local device id for this account (registered under urn:xmpp:omemo:2:devices)
    omemo_device_id INTEGER,
    display_name    TEXT,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now'))
);

-- ---------------------------------------------------------------------------
-- Roster (XEP-0237/0162) + presence cache
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS roster (
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    jid          TEXT    NOT NULL,                    -- contact bare JID
    name         TEXT,
    subscription TEXT    NOT NULL DEFAULT 'none',     -- none|to|from|both
    ask          TEXT,                                -- subscribe pending
    groups       TEXT,                                -- JSON array of group names
    PRIMARY KEY (account_id, jid)
);

CREATE TABLE IF NOT EXISTS presence (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    full_jid   TEXT    NOT NULL,                      -- bare/resource
    show       TEXT,                                  -- chat|away|xa|dnd|null(online)
    status     TEXT,
    priority   INTEGER NOT NULL DEFAULT 0,
    caps_hash  TEXT,                                  -- XEP-0115 ver
    updated_at TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, full_jid)
);

-- XEP-0115 entity capabilities cache (disco#info keyed by caps ver hash)
CREATE TABLE IF NOT EXISTS disco_caps (
    ver_hash TEXT PRIMARY KEY,                        -- e.g. "sha-1+<base64>"
    features TEXT NOT NULL,                           -- JSON array of feature vars
    identities TEXT,                                  -- JSON array
    cached_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ---------------------------------------------------------------------------
-- Conversations (1:1 + MUC) and bookmarks (XEP-0402)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS conversations (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    jid          TEXT    NOT NULL,                    -- contact or room bare JID
    kind         TEXT    NOT NULL DEFAULT 'chat',     -- chat|muc
    name         TEXT,
    -- per-conversation encryption mode: 'none' | 'omemo2'
    encryption   TEXT    NOT NULL DEFAULT 'none',
    muc_nick     TEXT,                                -- desired nickname for MUC
    muc_autojoin INTEGER NOT NULL DEFAULT 0,
    last_active  TEXT,                                -- timestamp of last message
    unread       INTEGER NOT NULL DEFAULT 0,
    archived     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (account_id, jid)
);

-- ---------------------------------------------------------------------------
-- Messages
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    -- XMPP stanza id / origin-id (XEP-0359) used for dedup, edits, receipts, reactions
    stanza_id       TEXT,                             -- server-assigned (by-id)
    origin_id       TEXT,                             -- sender-assigned (origin-id)
    counterpart     TEXT    NOT NULL,                 -- full or bare JID of the other party
    -- 'in' (received) | 'out' (sent)
    direction       TEXT    NOT NULL,
    body            TEXT,                             -- plaintext (post-decryption) body
    -- encryption used for THIS message: 'none' | 'omemo2'
    encryption      TEXT    NOT NULL DEFAULT 'none',
    -- delivery state for outgoing: queued|sent|received(0184)|displayed(0333)|error
    state           TEXT    NOT NULL DEFAULT 'queued',
    -- XEP-0308 edits: points at the message this one corrects (by stanza/origin id)
    edited_of       TEXT,
    -- XEP-0424 retraction tombstone
    retracted       INTEGER NOT NULL DEFAULT 0,
    -- XEP-0066/0363 attachment metadata (JSON: url, mime, size, width, height, sims hashes)
    attachment      TEXT,
    -- XEP-0461 reply-to (stanza id being replied to)
    reply_to        TEXT,
    -- OMEMO trust snapshot of the sending device at decrypt time
    omemo_fingerprint TEXT,
    timestamp       TEXT    NOT NULL DEFAULT (datetime('now')),  -- delay-corrected time
    received_at     TEXT    NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_messages_conv_time ON messages(conversation_id, timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_stanza ON messages(stanza_id);
CREATE INDEX IF NOT EXISTS idx_messages_origin ON messages(origin_id);

-- XEP-0444 reactions (separate so they can be updated/removed independently)
CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    reactor    TEXT    NOT NULL,                      -- bare JID of who reacted
    emoji      TEXT    NOT NULL,
    PRIMARY KEY (message_id, reactor, emoji)
);

-- XEP-0313 MAM sync cursors per archive (account or MUC)
CREATE TABLE IF NOT EXISTS mam_cursors (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    archive    TEXT    NOT NULL,                      -- '' for account archive, else MUC jid
    first_id   TEXT,                                  -- oldest known archive id
    last_id    TEXT,                                  -- newest known archive id
    complete   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, archive)
);

-- XEP-0191 blocking list
CREATE TABLE IF NOT EXISTS blocklist (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    jid        TEXT    NOT NULL,
    PRIMARY KEY (account_id, jid)
);

-- ===========================================================================
-- PQ OMEMO2 stores (urn:xmpp:omemo:2) — back the libsignal store traits.
-- Blobs are libsignal-serialized records; we keep XMPP addressing (jid+device).
-- ===========================================================================

-- Our own identity key pair per account (the long-term IK). Private half is sealed
-- in the secret service; this row holds the public IK + registration/device id.
CREATE TABLE IF NOT EXISTS omemo_own_identity (
    account_id    INTEGER PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    device_id     INTEGER NOT NULL,
    identity_pub  BLOB    NOT NULL,                   -- 33-byte curve pub (IdentityKey)
    -- libsignal IdentityKeyPair serialization is sealed in secret service; this flag tracks it
    has_private   INTEGER NOT NULL DEFAULT 0
);

-- Remote identities (TOFU trust). trust: 0=undecided 1=trusted 2=untrusted/blocked
CREATE TABLE IF NOT EXISTS omemo_identities (
    account_id   INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    address_jid  TEXT    NOT NULL,
    device_id    INTEGER NOT NULL,
    identity_key BLOB    NOT NULL,                    -- serialized IdentityKey (public)
    trust        INTEGER NOT NULL DEFAULT 0,
    seen_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    active       INTEGER NOT NULL DEFAULT 1,          -- present in latest device list?
    PRIMARY KEY (account_id, address_jid, device_id)
);

-- Double/triple-ratchet sessions (SessionStore)
CREATE TABLE IF NOT EXISTS omemo_sessions (
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    address_jid TEXT    NOT NULL,
    device_id   INTEGER NOT NULL,
    record      BLOB    NOT NULL,                     -- libsignal SessionRecord bytes
    PRIMARY KEY (account_id, address_jid, device_id)
);

-- Classic EC one-time pre-keys (PreKeyStore)
CREATE TABLE IF NOT EXISTS omemo_prekeys (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    prekey_id  INTEGER NOT NULL,
    record     BLOB    NOT NULL,                      -- libsignal PreKeyRecord bytes
    PRIMARY KEY (account_id, prekey_id)
);

-- Signed pre-keys (SignedPreKeyStore)
CREATE TABLE IF NOT EXISTS omemo_signed_prekeys (
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    signed_prekey_id INTEGER NOT NULL,
    record          BLOB    NOT NULL,                 -- libsignal SignedPreKeyRecord bytes
    PRIMARY KEY (account_id, signed_prekey_id)
);

-- PQ KEM pre-keys (KyberPreKeyStore) — the ML-KEM-1024 prekeys that drive PQXDH.
-- `used` implements the last-resort replay guard (markKyberPreKeyUsed semantics).
CREATE TABLE IF NOT EXISTS omemo_kyber_prekeys (
    account_id     INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kyber_prekey_id INTEGER NOT NULL,
    record         BLOB    NOT NULL,                  -- libsignal KyberPreKeyRecord bytes
    is_last_resort INTEGER NOT NULL DEFAULT 0,
    used           INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account_id, kyber_prekey_id)
);

-- Cache of remote device lists (urn:xmpp:omemo:2:devices) for quick fan-out
CREATE TABLE IF NOT EXISTS omemo_device_lists (
    account_id  INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    address_jid TEXT    NOT NULL,
    device_ids  TEXT    NOT NULL,                     -- JSON array of device ids
    fetched_at  TEXT    NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (account_id, address_jid)
);
