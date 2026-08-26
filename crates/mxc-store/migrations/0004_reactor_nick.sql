-- Display name of who reacted (MUC nick, "You", or a 1:1 contact), shown in the reaction
-- tooltip. The `reactor` key stays the stable id (occupant-id / bare JID) used for toggling.
ALTER TABLE reactions ADD COLUMN reactor_nick TEXT;
