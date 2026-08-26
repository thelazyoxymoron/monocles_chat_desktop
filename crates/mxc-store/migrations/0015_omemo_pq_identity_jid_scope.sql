-- Scope the post-quantum identity pin to the JID that owns the classical key.
--
-- `omemo_pq_identities` was keyed on (account_id, fingerprint) alone, with `address_jid`
-- carried along only for diagnostics. But a classical OMEMO identity key is published in PEP
-- for anyone to read, so pinning against the fingerprint alone let one JID poison another's
-- pin: publish someone else's <ik> beside your own <pq-ik>, get pinned on first contact, and
-- every later PQ OMEMO2 session with the real owner is refused as a changed pq_ik. That is a
-- durable denial of service against a contact, mountable by any peer. Mirrors the monocles
-- Android client's database v74.
--
-- The rows carry across as they are: `address_jid` already holds the JID we pinned from, which
-- is exactly the owner the new key wants. Where the old constraint had collapsed two owners
-- onto one row, the survivor keeps the owner it was last written for and the other simply
-- re-pins on its next bundle fetch (see `reconcile_pq_pin`), so nothing is lost but a fetch.

ALTER TABLE omemo_pq_identities RENAME TO omemo_pq_identities_old;

CREATE TABLE omemo_pq_identities (
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    address_jid     TEXT    NOT NULL,                 -- bare JID owning the classical key
    fingerprint     TEXT    NOT NULL,                 -- hex of the classical IdentityKey.serialize()
    pq_identity_key BLOB    NOT NULL,                 -- 2592-byte ML-DSA-87 verification key
    PRIMARY KEY (account_id, address_jid, fingerprint)
);

INSERT OR IGNORE INTO omemo_pq_identities (account_id, address_jid, fingerprint, pq_identity_key)
    SELECT account_id, address_jid, fingerprint, pq_identity_key FROM omemo_pq_identities_old;

DROP TABLE omemo_pq_identities_old;
