-- Per-conversation notification mode:
--   'all'              notify for every message (default)
--   'mentioned'        (MUC) notify only when our nick is mentioned
--   'mentions_replies' (MUC) notify on mentions and replies to our messages
--   'none'             never notify (muted)
ALTER TABLE conversations ADD COLUMN notify TEXT NOT NULL DEFAULT 'all';
