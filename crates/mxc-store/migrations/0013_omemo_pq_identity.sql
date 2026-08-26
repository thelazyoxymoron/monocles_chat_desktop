-- monocles PQ-OMEMO2 hybrid identity (proto-XEP §4.9): the post-quantum (ML-DSA-87) half
-- of each device's identity, used to authenticate published pre-key bundles so that an
-- active MITM of session establishment must break ML-DSA-87 as well as Ed25519.

-- Our own public ML-DSA-87 identity key (for displaying this device's hybrid fingerprint).
-- The secret half is sealed in the secret service alongside the classical identity.
ALTER TABLE omemo_own_identity ADD COLUMN pq_identity_pub BLOB;

-- TOFU pins of a peer's post-quantum identity, keyed to its classical identity. `fingerprint`
-- is the lowercase hex of the peer's serialized (33-byte) classical IdentityKey; `pq_identity_key`
-- is its 2592-byte ML-DSA-87 verification key. A *changed* pin for a known classical identity is
-- refused unless that classical fingerprint has been manually verified (see mxc-omemo).
CREATE TABLE IF NOT EXISTS omemo_pq_identities (
    account_id      INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    fingerprint     TEXT    NOT NULL,                 -- hex of the classical IdentityKey.serialize()
    address_jid     TEXT    NOT NULL,                 -- owning bare JID (diagnostics / UI)
    pq_identity_key BLOB    NOT NULL,                 -- 2592-byte ML-DSA-87 verification key
    PRIMARY KEY (account_id, fingerprint)
);
