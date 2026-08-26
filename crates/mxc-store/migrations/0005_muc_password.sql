-- Password for protected MUCs (XEP-0045). Kept locally so we can re-join on autojoin/restart;
-- XEP-0402 bookmarks don't carry a password, so it isn't synced across devices.
ALTER TABLE conversations ADD COLUMN muc_password TEXT;
