-- WebXDC (urn:xmpp:webxdc:0) support.
--
-- A WebXDC "app" is a `.xdc` (zip) shared in a chat; its instance is identified by the
-- `<thread>` UUID carried on the original file message. Participants then exchange *status
-- updates* (and ephemeral realtime data) that all reference that thread, syncing the app state.

-- The instance thread of a `.xdc` app message (NULL for ordinary messages).
ALTER TABLE messages ADD COLUMN thread TEXT;

-- Status updates for a WebXDC instance, ordered by a per-row serial (the cursor the JS API
-- pages from). `payload` is the app's JSON update; document/summary/info are optional metadata.
CREATE TABLE webxdc_updates (
    serial          INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id      INTEGER NOT NULL,
    thread          TEXT NOT NULL,
    message_id      TEXT,
    sender          TEXT,
    info            TEXT,
    document        TEXT,
    summary         TEXT,
    payload         TEXT,
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_webxdc_updates_thread ON webxdc_updates (account_id, thread, serial);
