//! Explicit private-input receiving; missing originals fail, never skip.
use semantica_runtime::sha256 as private_fixture_sha256;
mod loader {
    include!("../../../build_support/private_test_fixtures.rs");

    #[test]
    fn every_registered_original_retains_its_frozen_identity() {
        let root = FixtureRoot::from_environment().expect("explicit original input root required");
        // Preserve the accepted 129-entry sequence independently of every append.
        // Hash format is path + NUL + decimal bytes + NUL + lowercase SHA + LF.
        let identity_sequence = |entries: &[FixtureBinding]| {
            let mut bytes = Vec::new();
            for entry in entries {
                bytes.extend_from_slice(format!("{}\0{}\0{}\n", entry.path, entry.bytes, entry.sha256).as_bytes());
            }
            private_fixture_sha256(&bytes)
        };
        assert_eq!(CATALOG.len(), 129 + 3 + 8 + 4 + 7 + 5 + 3);
        assert_eq!(identity_sequence(&CATALOG[..129]), "1dd7bc6ba396ff0d1d63e9d8e4bc7db9955aa6807034ecd7220193e9ab42d364");
        assert_eq!(identity_sequence(&CATALOG[129..132]), "f8c6eabf8a3458d076143fc61896e56b2d6ab4b5bb38cc4970e8c6c1e7dd710c");
        assert_eq!(identity_sequence(&CATALOG[132..140]), "465629cc50755b9743696dfcc7a773d4b5e4393bfc7c8a4dd611cf716acea428");
        assert_eq!(identity_sequence(&CATALOG[140..144]), "ccd9918c89a8e0729baeffd0ab098b83531094999a71770a2b50317a22f67e09");
        assert_eq!(identity_sequence(&CATALOG[144..151]), "a003904610a1c180432517dc9ba5e8f683ba0adac5ac03cfd2ef536ee7808cd1");
        assert_eq!(identity_sequence(&CATALOG[151..156]), "731b38c4913ba34c6945d1409999aee3a94e2eec659e1c96962b194c838a5c93");
        assert_eq!(identity_sequence(&CATALOG[156..]), "753d6c6bc77a2485199d337786ec9212dcbc9122c14603e910fc5405a6f298a4");
        let distinct: std::collections::BTreeSet<_> = CATALOG.iter().map(|entry| entry.path).collect();
        assert_eq!(distinct.len(), CATALOG.len());
        for binding in CATALOG {
            let bytes = root.read(binding.path).unwrap_or_else(|e| panic!("{}: {e}", binding.path));
            assert_eq!(bytes.len() as u64, binding.bytes);
            assert_eq!(private_fixture_sha256(&bytes), binding.sha256);
        }
    }
}
