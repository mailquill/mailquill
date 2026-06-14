use api::imap_utf7::decode;

#[test]
fn decodes_german_umlauts() {
    assert_eq!(decode("J&APw-licher Str. 311"), "Jülicher Str. 311");
    assert_eq!(decode("B&APw-sbach"), "Büsbach");
    assert_eq!(decode("Kornelim&APw-nster"), "Kornelimünster");
}

#[test]
fn passes_through_plain_ascii() {
    assert_eq!(decode("INBOX"), "INBOX");
    assert_eq!(decode("Mobilfunk/Congstar"), "Mobilfunk/Congstar");
}

#[test]
fn literal_ampersand() {
    // "&-" is the encoding for a literal '&'.
    assert_eq!(decode("Rock &- Roll"), "Rock & Roll");
}

#[test]
fn malformed_runs_pass_through_without_panicking() {
    // No terminating '-' — emit the remainder verbatim.
    assert_eq!(decode("Bad&APw"), "Bad&APw");
    // Invalid base64 inside a run is kept raw rather than dropped.
    assert_eq!(decode("&@@@-x"), "&@@@-x");
}
