use slipway_core::{Authenticator, Hasher, PublicKey, Sha256Hex, SignError};

#[test]
fn digests_match_known_vectors_and_streaming() {
    assert_eq!(
        Sha256Hex::of(b"abc").as_str(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        Sha256Hex::of(b"").as_str(),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    let mut hasher = Hasher::new();
    hasher.update(b"a");
    hasher.update(b"bc");
    assert_eq!(hasher.finish(), Sha256Hex::of(b"abc"));
}

#[test]
fn digest_text_has_one_spelling() {
    assert!(Sha256Hex::parse(&"a".repeat(64)).is_ok());
    let (upper, odd, long) = ("A".repeat(64), "g".repeat(64), "a".repeat(65));
    for bad in ["", "abc", upper.as_str(), odd.as_str(), long.as_str()] {
        assert!(Sha256Hex::parse(bad).is_err(), "{bad}");
    }
}

// The key and signature below are the example from minisign-verify 0.2.5's own documentation
// (src/lib.rs): a prehashed signature of the four bytes `test`. They are a fixed test pair, not
// a key slipway ever trusts.
const TEST_KEY: &str = "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
const TEST_SIGNATURE: &str = concat!(
    "untrusted comment: signature from minisign secret key\n",
    "RUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\n",
    "trusted comment: timestamp:1633700835\tfile:test\tprehashed\n",
    "wLMDjy9FLAuxZ3q4NlEvkgtyhrr0gtTu6KC4KBJdITbbOeAi1zBIYo0v4iTgt8jJpIidRJnp94ABQkJAgAooBQ=="
);

#[test]
fn a_real_signature_verifies_and_a_changed_message_does_not() {
    let key = PublicKey::parse(TEST_KEY).expect("key");
    assert_eq!(key.authenticate(b"test", TEST_SIGNATURE), Ok(()));
    assert_eq!(
        key.authenticate(b"tesT", TEST_SIGNATURE),
        Err(SignError::NotSigned)
    );
    assert_eq!(
        key.authenticate(b"", TEST_SIGNATURE),
        Err(SignError::NotSigned)
    );
}

#[test]
fn a_key_file_with_a_comment_line_is_read_too() {
    let file = format!("untrusted comment: minisign public key 1\n{TEST_KEY}\n");
    assert_eq!(PublicKey::parse(&file), PublicKey::parse(TEST_KEY));
}

#[test]
fn bad_keys_and_signatures_are_refused() {
    for bad in [
        "",
        "not a key",
        "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO",
    ] {
        assert_eq!(PublicKey::parse(bad), Err(SignError::BadKey), "{bad:?}");
    }
    let key = PublicKey::parse(TEST_KEY).expect("key");
    for bad in [
        "",
        "garbage",
        "untrusted comment: x\nAAAA\ntrusted comment: y\nBBBB",
    ] {
        assert_eq!(
            key.authenticate(b"test", bad),
            Err(SignError::BadSignature),
            "{bad:?}"
        );
    }
}
