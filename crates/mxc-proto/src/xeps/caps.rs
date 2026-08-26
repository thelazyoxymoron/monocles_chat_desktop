//! XEP-0115 entity capabilities: compute the `ver` hash over our disco#info so peers
//! can cache our feature set, and include `<c/>` in outgoing presence.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use minidom::Element;
use sha1::{Digest, Sha1};

use crate::xeps::disco;

pub const NODE: &str = "https://monocles.eu/chat";

/// Compute the XEP-0115 verification string (S) and its base64 SHA-1 hash.
///
/// S = identities (sorted) + features (sorted), each `key<` separated, per §5.1.
pub fn ver_hash() -> String {
    let mut s = String::new();
    // single identity: category/type/lang/name
    s.push_str(&format!("{}/{}//{}<", disco::CLIENT_CATEGORY, disco::CLIENT_TYPE, disco::CLIENT_NAME));
    // FEATURES is kept pre-sorted in disco.rs
    for f in disco::FEATURES {
        s.push_str(f);
        s.push('<');
    }
    B64.encode(Sha1::digest(s.as_bytes()))
}

/// The `<c/>` element to attach to presence.
pub fn caps_element() -> Element {
    Element::builder("c", "http://jabber.org/protocol/caps")
        .attr(crate::ncname("hash"), "sha-1")
        .attr(crate::ncname("node"), NODE)
        .attr(crate::ncname("ver"), ver_hash())
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash must be a real SHA-1/base64 of the verification string (28-char base64 of 20
    /// bytes), not the old constant stub — peers verify it before trusting our disco features.
    #[test]
    fn ver_hash_is_real_sha1() {
        let v = ver_hash();
        assert_eq!(v.len(), 28, "sha1 base64 is 28 chars, got {v:?}");
        assert_ne!(v, "AAAAAAAAAAAAAAAAAAAAAAAAAAA=", "still the placeholder stub");
        // The video feature must be in the set the hash is computed over.
        assert!(disco::FEATURES.contains(&"urn:xmpp:jingle:apps:rtp:video"));
    }

    /// Sanity-check the primitive against a known SHA-1 vector (sha1("test")).
    #[test]
    fn sha1_base64_primitive() {
        assert_eq!(B64.encode(Sha1::digest(b"test")), "qUqP5cyxm6YcTAhz05Hph5gvu9M=");
    }
}
