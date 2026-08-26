-- Last-resort KEM prekey replay tracking (proto-XEP OMEMO-PQXDH §6.4).
--
-- The signed (last-resort) KEM prekey is reused across session initiations, so a
-- malicious server could replay a captured PreKeySignalMessage that used it. We
-- record the (kyber_prekey_id, signed_prekey_id, base_key) tuple of every
-- last-resort session initiation and reject duplicates during decryption
-- (mirrors Android's kyber_last_resort_sessions table / ReusedBaseKeyException).
CREATE TABLE IF NOT EXISTS omemo_kyber_last_resort_sessions (
    account_id       INTEGER NOT NULL,
    kyber_prekey_id  INTEGER NOT NULL,
    signed_prekey_id INTEGER NOT NULL,
    base_key         BLOB    NOT NULL,
    PRIMARY KEY (account_id, kyber_prekey_id, signed_prekey_id, base_key)
);
