-- The current MUC subject/topic (XEP-0045 §8.1), captured from live groupchat <subject>
-- messages. Distinct from disco#info's short muc#roominfo_description.
ALTER TABLE conversations ADD COLUMN muc_subject TEXT;
