-- ---------------------------------------------------------------------------
-- Encrypted MUC support (PQ OMEMO2 in group chats).
--
-- OMEMO in a MUC is only possible when the room is "private and non-anonymous"
-- (members-only + non-anonymous, matching monocles Android's
-- MucOptions.isPrivateAndNonAnonymous): only then do we learn each member's real
-- bare JID, which is what we encrypt to. Cache the two room features so the UI
-- can gate the lock toggle, and track occupants' real JIDs/affiliations so we can
-- (a) build the crypto-target member list on send and (b) resolve a groupchat
-- sender's real JID on receive.
-- ---------------------------------------------------------------------------

ALTER TABLE conversations ADD COLUMN muc_members_only  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE conversations ADD COLUMN muc_non_anonymous INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS muc_occupants (
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    nick            TEXT    NOT NULL,   -- occupant nick (the resource of room@host/nick)
    real_jid        TEXT,              -- real bare JID (non-anonymous rooms only)
    affiliation     TEXT,              -- owner|admin|member|none|outcast
    PRIMARY KEY (conversation_id, nick)
);

CREATE INDEX IF NOT EXISTS idx_muc_occupants_real_jid
    ON muc_occupants (conversation_id, real_jid);
