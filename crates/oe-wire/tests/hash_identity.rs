#[test]
fn sha256_known_answers_keep_exact_lowercase_identity() {
    let cases = [
        (
            Vec::new(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc".to_vec(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            vec![b'a'; 1_000_000],
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(oe_wire::sha256(input), expected);
    }
}
