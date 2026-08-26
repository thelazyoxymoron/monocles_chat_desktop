-- Call history (audio/video calls placed and received).
CREATE TABLE call_log (
    id         INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    peer       TEXT    NOT NULL,            -- bare JID of the other party
    direction  TEXT    NOT NULL,            -- 'in' | 'out'
    video      INTEGER NOT NULL DEFAULT 0,  -- 1 = video call
    answered   INTEGER NOT NULL DEFAULT 0,  -- 1 = the call connected
    timestamp  TEXT    NOT NULL             -- RFC3339 start time
);
CREATE INDEX call_log_account_time ON call_log (account_id, timestamp DESC);
