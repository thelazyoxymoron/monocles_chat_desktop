-- XEP-0421 occupant ids, needed for MUC reactions (XEP-0444): reactions in a group chat are
-- attributed to the stable occupant id rather than the (spoofable, changeable) nick.
ALTER TABLE messages ADD COLUMN occupant_id TEXT;
-- Our own occupant id within a MUC, learned from the self-presence (status 110) / reflected
-- messages, so we can attribute and toggle our own reactions consistently.
ALTER TABLE conversations ADD COLUMN muc_self_occupant_id TEXT;
