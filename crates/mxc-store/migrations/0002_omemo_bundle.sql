-- Cache our published OMEMO2 bundle (its <bundle> XML) so it can be re-published verbatim
-- on startup instead of regenerating the pre-keys every run (which rotated them).
ALTER TABLE omemo_own_identity ADD COLUMN bundle_xml TEXT;
