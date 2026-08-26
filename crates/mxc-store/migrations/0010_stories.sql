-- Social-feed Stories (XEP pubsub-social-feed:stories:0): ephemeral 24h media posts from
-- contacts (and ourselves), cached locally. `published` is unix seconds.
CREATE TABLE stories (
    uuid       TEXT PRIMARY KEY,
    account_id INTEGER NOT NULL,
    contact    TEXT    NOT NULL,            -- publisher bare JID
    url        TEXT    NOT NULL,            -- media URL (https or aesgcm)
    type       TEXT    NOT NULL,            -- MIME type
    title      TEXT,
    published  INTEGER NOT NULL
);
CREATE INDEX stories_account_pub ON stories (account_id, published DESC);
