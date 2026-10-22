// Shared test-only loader. Frozen payloads stay outside the public source tree.
// Identity catalog: ALL-TARGET-FIXTURE-RESTORED-01 plus the four additional
// compile inputs recorded in PUBLIC-FIXTURE-PACKAGING-REVIEW-01 (private receipts).
// Unicode originals additionally retain PRIVATE-TEST-INPUT-HANDOFF identities;
// their two-file restoration is recorded in UNICODE-PRIVATE-INPUT-RESTORATION-01.
// The caller supplies its existing SHA-256 implementation; no new runtime owner.
use super::private_fixture_sha256;
use std::{
    ffi::OsStr,
    fs::{File, Metadata},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

const ROOT_ENV: &str = "OE_PRIVATE_TEST_FIXTURE_ROOT";

#[derive(Clone, Copy, Debug)]
struct FixtureBinding {
    path: &'static str,
    bytes: u64,
    sha256: &'static str,
}

const CATALOG: &[FixtureBinding] = &[
    FixtureBinding { path: "source-session-48/original.json", bytes: 1177407, sha256: "5cf9d5eb482a901a50aef3fe56c8984496965b5a954894f2854a89d0c46a0651" },
    FixtureBinding { path: "source-lifecycle-46/original-complete.json", bytes: 4464260, sha256: "2abb516f7dabf70ec24e3756749e8f832c484c6d4af26902c9c32e65857aa5b3" },
    FixtureBinding { path: "source-lifecycle-46/cold.json", bytes: 476732, sha256: "47f700b5b1c5f8b843ae57d38560f51e810b28b9e79b2d217643b22bcacbd9f2" },
    FixtureBinding { path: "source-lifecycle-46/original.json", bytes: 4463882, sha256: "c874988b9b9a6296d70262ad2cc3e024593959fc67529443d868fc1d4e9e6177" },
    FixtureBinding { path: "source-namespace-45/query_errors.json", bytes: 166568, sha256: "46e3688d086543349d503e130958b14a4c8b1d6cacb969ecffbd51cccdffeba1" },
    FixtureBinding { path: "source-namespace-45/original.json", bytes: 1821484, sha256: "e9e05863822aaa1bf97e14ee8e8f7e4f33231992ce8911d875cba646af7c0698" },
    FixtureBinding { path: "source-namespace-45/supplement.json", bytes: 528099, sha256: "b5a466466d2886e72b8aedb11dc34b2989c495b8d17d7bba2afaebfbd5e00aa9" },
    FixtureBinding { path: "source-namespace-45/controls.json", bytes: 172897, sha256: "46dde46e3b8c3bdd7200ad2e65e620305f6b93a219f268f07719adbff9dd36fc" },

    FixtureBinding {
        path: "source-turtle-44/supplement.json",
        bytes: 39851,
        sha256: "aaf87cae24139e14395a90d2d9fc74cb87167fe9610c59c70d94adaaa6cdef95",
    },
    FixtureBinding {
        path: "source-turtle-44/original.json",
        bytes: 323899,
        sha256: "4dcc176e4e3846ea1473df506f64f3a43ca6a797c73db2d8baf69c92c7d7cb05",
    },
    FixtureBinding {
        path: "source-turtle-43/original.json",
        bytes: 391039,
        sha256: "36e8735cd217a4b5804907a166cf393d5e073a918580ffd25c2a9566ffd5ff02",
    },
    FixtureBinding {
        path: "ORIGINAL-PYTHON-UNICODE-CLASSIFICATION.bin",
        bytes: 1114112,
        sha256: "b79eaa6372a6d75fcd37311560beff4e26aa3b1c2f1c5f7a721a9b00e52bde10",
    },
    FixtureBinding {
        path: "ORIGINAL-PYTHON-UNICODE-DECIMAL.json",
        bytes: 23002,
        sha256: "2cbf99cb55b4046965919a6467dca68736620b7d94b363050b20297cc26416b3",
    },
    FixtureBinding {
        path: "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-INPUTS-AND-EXPECTED-01.json",
        bytes: 22528,
        sha256: "492039a3d723d94e8190cf326483c8afa8a2b7dc03421cd892c3c81bd22225b9",
    },
    FixtureBinding {
        path: "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-LIB-SOURCE-EXPECTED-02.json",
        bytes: 3261,
        sha256: "051d4c532f4f222748be815deeeb8dbd7325cf74fd027538c52a208b13471c19",
    },
    FixtureBinding {
        path: "FIXED-CARGO-TARGET-QUERY-FAMILY-06/FIXED-PURE-PREDICATE-CONTEXT-01.json",
        bytes: 14259,
        sha256: "4e1d6e7607a7c46f60615a84a62d8f3b8e5d6224d42b9c27afe1feeef5e96a98",
    },
    FixtureBinding {
        path: "FROZEN-CURRENT-EXPECTATIONS-01.json",
        bytes: 2205,
        sha256: "519b0055644405e37e02daf60b2e12df6085456bd56ba7534d4638db931a95e6",
    },
    FixtureBinding {
        path: "literal-eight/ORIGINAL-INPUTS-01.json",
        bytes: 39950,
        sha256: "0364e11610abc2e7e2ef68944093408d5800f17e4a1ade89a02d39015d5c2f02",
    },
    FixtureBinding {
        path: "literal-eight/ORIGINAL-ORACLE-01.json",
        bytes: 199558,
        sha256: "c6055f9eb11a907a4dd062efe5d03315877a4f8afe18fc9556d6e55415e31d69",
    },
    FixtureBinding {
        path: "literal-eight/SUPPLEMENT-INPUTS-01.json",
        bytes: 51372,
        sha256: "2dfe02d8f42e2e82a1e3defea7c588c6fa1e10c7ec937863230206a2b8c65a19",
    },
    FixtureBinding {
        path: "literal-eight/SUPPLEMENT-ORACLE-01.json",
        bytes: 231326,
        sha256: "1e2844b46a0b2ffac161a9eba116b61c44da4b5cf5474c215369650fd23756e7",
    },
    FixtureBinding {
        path: "literal-xml-html/BINDING-ORIGINAL-FROZEN-INPUTS.json",
        bytes: 3728,
        sha256: "5422fa04cd4441f8b26cb6e83497e5648b3b786e3c4c28467164a16fe660d2db",
    },
    FixtureBinding {
        path: "literal-xml-html/BINDING-ORIGINAL-FROZEN-ORACLE.json",
        bytes: 12326,
        sha256: "1fff14a5b500d396fd25cf58d32f7c1d6a5e5080574fb5a7616eaeca0dfee207",
    },
    FixtureBinding {
        path: "literal-xml-html/EXTRA-ORIGINAL-FROZEN-INPUTS.json",
        bytes: 23723,
        sha256: "b64704a32907a60c369f07c383ae7faec8c6d347cda161368ef2c82e548fd035",
    },
    FixtureBinding {
        path: "literal-xml-html/EXTRA-ORIGINAL-FROZEN-ORACLE.json",
        bytes: 91777,
        sha256: "e8ef8f1787579c87b9cbd8f44022a70f9307ee4fda161a5c060408b746363a6c",
    },
    FixtureBinding {
        path: "literal-xml-html/ORIGINAL-FROZEN-INPUTS.json",
        bytes: 41115,
        sha256: "59a754dd427a2caff0dd02d29332e4cfd31a7e2c011354aae1da5d4f29dff6f2",
    },
    FixtureBinding {
        path: "literal-xml-html/ORIGINAL-FROZEN-ORACLE.json",
        bytes: 154568,
        sha256: "f1cf869a2bffcdcc68ca2225c5aaee4ced8302945f6f36201ee2a6179f5f0dfc",
    },
    FixtureBinding {
        path: "literal-xml-html/POLICY-ORIGINAL-FROZEN-INPUTS.json",
        bytes: 38215,
        sha256: "08eafa6065396b59c2b842371c699ebd2a625eaa733fed8d0be2cd0ba6707812",
    },
    FixtureBinding {
        path: "literal-xml-html/POLICY-ORIGINAL-FROZEN-ORACLE.json",
        bytes: 141798,
        sha256: "3d9fcda841f142b605a14c9be42b27de12c64facc1f8a01991b583cac9597856",
    },
    FixtureBinding {
        path: "normalizer-review/CONSTRUCTOR-INPUTS.json",
        bytes: 3983,
        sha256: "7756bcb361a7cb403d0adab96e28292ed43eb682c06333590c4f7f0b6da81be3",
    },
    FixtureBinding {
        path: "normalizer-review/CONSTRUCTOR-ORACLE.json",
        bytes: 19126,
        sha256: "5c9c4ac3534d43b944c9709fb72d4651bd71e9c0cca39515c0f741ac1f559feb",
    },
    FixtureBinding {
        path: "original/BOUNDARY-FROZEN-INPUTS.json",
        bytes: 5340,
        sha256: "a955f32b3cefdf463769ef1c6a97937d523b36e3490d628ae8a8400bd3d0d7cc",
    },
    FixtureBinding {
        path: "original/BOUNDARY-FROZEN-ORACLE.json",
        bytes: 16838,
        sha256: "2e1f3b735c3fa807a44142ad3d27b7d311c9f9ebf0acd496de29aa4e6b1652b0",
    },
    FixtureBinding {
        path: "original/EXTRA-FROZEN-INPUTS.json",
        bytes: 22972,
        sha256: "16d1f7b723a1023f25490ab936bed3140330412f7f96a2899796c400393c8264",
    },
    FixtureBinding {
        path: "original/EXTRA-FROZEN-ORACLE.json",
        bytes: 223707,
        sha256: "1f352efc9b1d6d6434bf855f588b8fe1aa4698a8b40706c5689280c0a96955af",
    },
    FixtureBinding {
        path: "original/INTEGER-LIMIT-FROZEN-INPUTS.json",
        bytes: 27160,
        sha256: "6d6ae770691c7fe70852cb473714f95bedeeb6b024901a7d0f85ee39b7ed72ab",
    },
    FixtureBinding {
        path: "original/INTEGER-LIMIT-FROZEN-ORACLE.json",
        bytes: 28788,
        sha256: "8b417643ece265486570e7c1b0d540ca8b2c4d89e3a915346d2f7ef810f1a21e",
    },
    FixtureBinding {
        path: "original/LANGUAGE-SYMMETRY-OBSERVATION-INPUTS.json",
        bytes: 392,
        sha256: "c085e5214429b7f3bf97662e39385bcabc1f6092d56c606fc74fe8342834f856",
    },
    FixtureBinding {
        path: "original/LANGUAGE-SYMMETRY-OBSERVATION.json",
        bytes: 45964,
        sha256: "42c18d0e37054991ffff13e004ddc531b68a0c30ca784478b014b2997ca8afbe",
    },
    FixtureBinding {
        path: "original/ORIGINAL-CONSTRUCTOR-BOUNDARY-INPUTS.json",
        bytes: 2971,
        sha256: "94a1094140458073d994862eb8bc52fe77e86d93cd2f5f512c234133d642f4de",
    },
    FixtureBinding {
        path: "original/ORIGINAL-CONSTRUCTOR-BOUNDARY-ORACLE.json",
        bytes: 30941,
        sha256: "9ab2b500fefdaf36861d56107b290fc41d0c1cdc8f67b235b505ad77445a8a83",
    },
    FixtureBinding {
        path: "original/ORIGINAL-FROZEN-INPUTS.json",
        bytes: 14040,
        sha256: "868fc20bf19f1f31d3fa2392a00b2e0a5d605ebe3226c3d4f744bdc42201f4d2",
    },
    FixtureBinding {
        path: "original/ORIGINAL-FROZEN-ORACLE.json",
        bytes: 161613,
        sha256: "978699b8085dd92084c3074671838b793b2b638ec0f52c1cd78212dd04b60123",
    },
    FixtureBinding {
        path: "original/ORIGINAL-PROVENANCE-BOUNDARY-INPUTS.json",
        bytes: 13436,
        sha256: "40444b9beb2c7b2a3bb7b9a817102d16ef723c3e73f61077bc72580ebf2c253d",
    },
    FixtureBinding {
        path: "original/ORIGINAL-PROVENANCE-BOUNDARY-ORACLE.json",
        bytes: 163646,
        sha256: "c042f47bfe0d7a79df6d39e0fe499a90bfaf891d81f45d50a0dd6ff3aff4c6a9",
    },
    FixtureBinding {
        path: "original/ORIGINAL-PURE-PROVENANCE-INPUTS.json",
        bytes: 2868,
        sha256: "e68a78306a410b96dcdfc0ede96c6c92658833d2b5537dfdd4d357a77f20e8eb",
    },
    FixtureBinding {
        path: "original/ORIGINAL-PURE-PROVENANCE-ORACLE.json",
        bytes: 87966,
        sha256: "21e6d54b9b8be327eee4f53bcd33a29f15681753b8c96f0a491055a09ee45c5c",
    },
    FixtureBinding {
        path: "original/full12/INPUT-actual-positive-case-single-fault.json",
        bytes: 44739,
        sha256: "a80157642ddcdcfbc05f4e4af4c357acafe50c17d6ecdf5ab63a8c93febd2d46",
    },
    FixtureBinding {
        path: "original/full12/INPUT-nonzero-immutable-baseline-valid-ontology-replacement.json",
        bytes: 36690,
        sha256: "2b72eca690132ccf0823022081a6de5048a7a30d782d7f91589881a756d15175",
    },
    FixtureBinding {
        path: "original/full12/INPUT-nonzero-whole-CQ-registry-replacement-does-not-waive-prior-CQ.json",
        bytes: 40623,
        sha256: "8198bf6601c53d2ed6602dd1d8501a0f45a71cc59e9dbe339aaeb213a22411de",
    },
    FixtureBinding {
        path: "original/full12/ORIGINAL-FULL12-FIXED-PLAN.json",
        bytes: 54548,
        sha256: "e364a710b4921a17c27360e325e21c9a26a931c45c6c49ccaf3cfd2d0e624f41",
    },
    FixtureBinding {
        path: "original/full12/ORIGINAL-FULL12-ORACLE.json",
        bytes: 2752263,
        sha256: "002b312aaa020ea2782113c4a614f5dfc579b8b2f1d85426b9f72026e3fba99d",
    },
    FixtureBinding {
        path: "planning-registry-overlay/ORIGINAL-INPUTS-01.json",
        bytes: 160204,
        sha256: "b14678815dca4c07492c5e7f0b5d32799c3395c387a2243010ddf29a3b5a6f1a",
    },
    FixtureBinding {
        path: "planning-registry-overlay/ORIGINAL-ORACLE-01.json",
        bytes: 57396,
        sha256: "b112d7272130e8e6c4658c46ff0ee7f9a10e0a1b77bc37c3ad90bf1bdcb623a2",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-jsonld-original.json",
        bytes: 233965,
        sha256: "46cc834b81b46f8a978eef100d5e537b3f84ccceb1577383233f4ac47728b61f",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-persistent-session-original.json",
        bytes: 505886,
        sha256: "31f784753857d28cfefe211ed159c1d1c807132833b3103961af8078de080274",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-rdf-duplicate-original.json",
        bytes: 3817,
        sha256: "68ef73b5f80c1ee478e5032463d636cee3592801e57bc321c935c9da36075853",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-rdf-formats-original.json",
        bytes: 178007,
        sha256: "d21101fd3000371ed93e08c44caf03f969d15bef088b84db1a72dcfbdd780daf",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-rdf-loading-original.json",
        bytes: 874158,
        sha256: "7e1fc6dd5ae195c038a3329cd44093ab8bd2080a0bd40ad9f09cc69decde0baf",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-session-helpers-original.json",
        bytes: 103647,
        sha256: "5ce2eed541ba88f3522f36da84496f3395df81759f5e990c8fffa3429a5ace21",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-trix-construct-original.json",
        bytes: 362041,
        sha256: "e6cf066345c4820aefbc58a15e7e677ca55a62d7f507c72c38c3ea10bcb30690",
    },
    FixtureBinding {
        path: "release-inputs-03/crates/semantica-runtime/tests/fixtures/source-update-original.json",
        bytes: 1964360,
        sha256: "7bb3c73aebd8725aaaafa1bebfcf54fb1d1c0f1ed3ed27d0bcf0f9673d258895",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_01.json",
        bytes: 3459493,
        sha256: "bd8ba93b4f51de1472e1536c985ef9ca203a8bad84d3b66fe425adedd3b4102c",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_02.json",
        bytes: 3335321,
        sha256: "c607228b53f88fe0c253fc7a73d91543400999d23d6189c857c76ff52942d8a4",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_03.json",
        bytes: 3100550,
        sha256: "b35d734508dc2bf4d5c01121ff275e376eeeb4a96cf3441ba57acca7745dd7bd",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_04.json",
        bytes: 3184041,
        sha256: "19b6a53e462d263b728cf66273e0f9eb47d568920aabda23b0f06df255d2d615",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_context_query_cases_05.json",
        bytes: 2122543,
        sha256: "e856b7aff66a19bbcaa4919b353ef3a9d0abf6113f7bf94012367038df414fb0",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_01.json",
        bytes: 74592,
        sha256: "6064e85e65fd6a4871b52061b131837bf952f8055c20b57989d623fcfd349930",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_02.json",
        bytes: 131108,
        sha256: "35a77b6d3123a674ec33102016ac1062f8180f115c5eb46ef7e8e1a622062845",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_03.json",
        bytes: 145496,
        sha256: "f38e05c155dec83a8f815868a8e7f18e0798a330bc4e705e70730ca8113b32db",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_04.json",
        bytes: 172760,
        sha256: "4788319a610b84c979d51c76ac2422f7070e663fdedd3bda5792b3404c30c3f9",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_05.json",
        bytes: 135884,
        sha256: "8167c22c12c328ca31052a6dca884031359fbf36fba8e8ad682d7583904cff90",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_06.json",
        bytes: 168596,
        sha256: "d6bca8a34e752d66461b6c2d0d071bf882009269a544948baec02afb4c29a039",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_calendar_duration_query_cases_07.json",
        bytes: 83499,
        sha256: "454e5c8a151626f370ff7ecdd59a45df63b62cb78b7a3443ec917b5db54a9078",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_coalesce_query_cases.json",
        bytes: 221152,
        sha256: "5e81b804d7475c03eeadbfb35f5e03165237cfcac9c69739dd1429872292e6d1",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_01.json",
        bytes: 2206889,
        sha256: "a0d4fe94e80a1efcae7022acf59cca24ef36baa9f7f8359dc131c5f095e5b65a",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_02.json",
        bytes: 2170367,
        sha256: "1a7f52d3f216e541a48a3ce97067fd6a638945b1da93c3c330b640a7a2083467",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_03.json",
        bytes: 2209701,
        sha256: "159df01d155e6aeed67aa7bcdfe7fbe03486a1031946b8abfb41cf5535563114",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_comparison_exception_query_cases_04.json",
        bytes: 2998200,
        sha256: "b5baa050e615ee6bd5a3b3e0c2dbabf4e1b32f26d6d8acf33a8e47338475a31f",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_constructor_identity_datatype.json",
        bytes: 2232709,
        sha256: "4c8470d73bc435ae9e443cacd4ee74d848959f3032f9e0c678d7763eddf9da79",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_constructor_identity_repeats.json",
        bytes: 367913,
        sha256: "5fcda18c388765cb302201cc98e4d1102ff88a9d295a61dd36035b7775b95a6c",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_constructor_identity_supplement.json",
        bytes: 541230,
        sha256: "ea5172d389be123635a7989b309f1e4693acdc7d8168c3c7627b3b3de1762ffa",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_01.json",
        bytes: 2013725,
        sha256: "e153e7383a7f3780b80769ce2754a787b92d9c7bf838b9c6c6ed3afb890da668",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_02.json",
        bytes: 2013653,
        sha256: "363f25b85ca5dd72477d9f21cd8f1a81acefb17c491d4e1923a4631aaca94152",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_03.json",
        bytes: 2019989,
        sha256: "b80c5a55268d405ed6b6fd05d38447009b2ed553467a62192c0aa9f548a2c41b",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_04.json",
        bytes: 1977665,
        sha256: "5c1d21401bca9b5da76dc553fbdc0a66b99eafb3362a9b730d14a4c20ba7b97a",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_order_query_cases_05.json",
        bytes: 2514788,
        sha256: "d6b9e4255771bc048b592861df5ab5ad8d75bd7ce76434f9e04ab43cf1993a47",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_datetime_query_cases.json",
        bytes: 1584886,
        sha256: "82791ae7821ba85f4be7a882fa3dc5b472cc8b6475a6b9c4eaf37c38d80e9117",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_decimal_trap_query_cases.json",
        bytes: 2849307,
        sha256: "c0e2fe2466920cf74fab37928f89a7cd4a11560d0e4ddfc894b684db4ddc1285",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_decimal_trap_str_query_cases.json",
        bytes: 164166,
        sha256: "c3b62e098bc95b4685ec87b84ebae5e837eddd62e17569f92813b6ec0891660e",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_decimal_trap_text_query_cases.json",
        bytes: 172483,
        sha256: "c0200ce33b488f818a10d317f1cd7125633f32db27ff94df709b9de90f5c7826",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_exception_query_cases.json",
        bytes: 1512141,
        sha256: "998dc1f356afba6edd28989f4d4685a5fc700fc1bf6977d55c21132beb1bbd56",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_registry_query_cases.json",
        bytes: 2062831,
        sha256: "0a36d7cce09e2dcc88fbf781958003b9d086c376c337e619b8ec0374bc51328d",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_function_runtime_query_cases.json",
        bytes: 1548698,
        sha256: "476c6ddb222c75b8f86820d2da81b36b6497bfad7ed3277e9df856006d31e59a",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_group_base_query_cases.json",
        bytes: 981243,
        sha256: "baa9b4d1f3ab2cd0fc2734fc1935fc4bba29c3a02255656c8aae38fc7b34774c",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_consumers_01.json",
        bytes: 3117147,
        sha256: "4f02882ed3825db71f1e6aa2bd4ee4b5f40812d92484152a3fbe3322f972fe44",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_01.json",
        bytes: 1735793,
        sha256: "cc7ce99509909f32b1d867f338265c8b284caedc0e89593e37a59e29678fc2e2",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_02.json",
        bytes: 1741074,
        sha256: "3a77f9bd649a2db5d846ec1c37393918d4b6a4dfc874b819ed5f6da1efd954c8",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_03.json",
        bytes: 1739574,
        sha256: "aa33550602abfc48524eb3f275261a074681224e854bf919b6ec45ab6a05beb3",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_initial_value_query_cases_04.json",
        bytes: 1737442,
        sha256: "55c40545acc91207904e59c55eeff027ad383f083d557947589d496310d1f13c",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_native_containers_query_cases.json",
        bytes: 924253,
        sha256: "e3cf6d83abdcd67c022793a55507555561fff0541915b4e88754b823fcafea77",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relation_negation_query_cases_01.json",
        bytes: 1006931,
        sha256: "0694d90933067b26df1f19f1829466599e134bc4a0746f2db4e5fb5a960a6021",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_01.json",
        bytes: 2180220,
        sha256: "88cda05a610f161cee2b40a8ce6d8376e4cf659e0a44c59bcecfe70a357c15b7",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_02.json",
        bytes: 2046536,
        sha256: "0d42f06b8d8ac690b92ec222bf5b53be623b60feffdbd67a2cafe09d811b677a",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_03.json",
        bytes: 2149570,
        sha256: "2ab2b10c5e68b27fc130b94e31ac55d26fc5b9da7030f303503fa9819498c093",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_relational_value_query_cases_04.json",
        bytes: 2264722,
        sha256: "26b9556aa568c6b41e7bef714409821a38769e5ab51ddc7e5770888aa8612e10",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_runtime_initial_original.json",
        bytes: 8588,
        sha256: "8b9dd534fd3499641334744a83f7d06b97a07e9a9c5517fdae4586f5b7f07bd0",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_runtime_results_original.json",
        bytes: 26339,
        sha256: "12d5aa30195547506d010fab063166f353e8ba2e10208b45b3910b0c070b2df7",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_syntax_protocol_query_cases.json",
        bytes: 2652085,
        sha256: "24c6bf443497e7311b660deada16af9bc49f12ec7bf5344cf5679d19710c6536",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_01.json",
        bytes: 2957094,
        sha256: "328cb3a33e9ed3a874457ae4f404f7215b59bb23159d54aec41d13b03e661e28",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_02.json",
        bytes: 2970220,
        sha256: "450aa6d17e88e1ec9f6759dd776e32e44576dfb1c60f32ab27d72f1d08a483af",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_03.json",
        bytes: 2268716,
        sha256: "51fa6c4dd4668db495a7576eb2c0f285751c9087350dc7478b6b38ab5c9b40bc",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_04.json",
        bytes: 2732414,
        sha256: "3046844a5c8c8a724983f08d8d33eb19d601fd0c7f2bb413dbd5be7fca3b0159",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_05.json",
        bytes: 3449932,
        sha256: "e457df67040cc567b21a32ac784c4d9a4719a960a9931bb2099685fb57d18288",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_query_cases_06.json",
        bytes: 448384,
        sha256: "ae1d0ff060bfbc7c223a83c3a5c206cc672797ecef6223fb7433d4ada044fdda",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_difference_recovered_01.json",
        bytes: 662057,
        sha256: "cbcb536f509b000cdf48391e391762eae4ccbd9cc03a6565b8229636808caac8",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_constructors_01.json",
        bytes: 7588,
        sha256: "21b77dfffbc4c9c06c6161b7fa4d4a7c1b60235d08710f3ea3044750d221bff4",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_01.json",
        bytes: 56821,
        sha256: "3a2720e50940e2c8bb0571e613d66913badeae1e4cb91d0553731c35e2ad7f0d",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_02.json",
        bytes: 61029,
        sha256: "7f69393cf580d96a96909c69c79f0e9901fbf7ee158a7f1ff4098503e86cff87",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_03.json",
        bytes: 67853,
        sha256: "f3f826e58671e6485dcf1934010f42b038003e93b0685112a6c5eaafe9933bd7",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_domain_query_cases_04.json",
        bytes: 40309,
        sha256: "ef21b91e9f1e2db64239b8ec5742c18957c22db113a8ad10e2c1ede2c7ccc0e2",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_01.json",
        bytes: 30132,
        sha256: "9ced4111b2e7d37c81d8dfbfcbb3ca19d127a77adc5fade756312b2d312e0a9e",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_02.json",
        bytes: 31288,
        sha256: "5d94ac49e8f609917252411e0c79e111e7a2d4c07ceec5c2d15cd99ff567af9d",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_03.json",
        bytes: 30764,
        sha256: "860b5c6f2abc43f874087b92b10f1029ce64b08171624be38e83a1573dc00852",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_04.json",
        bytes: 30368,
        sha256: "4503654099ce13737d683e376ce6cf51d08c39960de7b520c39e4439eee36f9b",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_05.json",
        bytes: 29288,
        sha256: "d98a49897358426004bb080f92cff0388fbef7c71c84676b8a593c6da648b8c3",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_06.json",
        bytes: 35520,
        sha256: "4430fbed203c2a36f4b02764398d49eab29912bbbd5c781756bddc7fdf55bd67",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_query_cases_07.json",
        bytes: 17419,
        sha256: "d041d4d3b671a0f188f53b129eb1fd117bfb801b93c3d4fe839356678945cdcf",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_temporal_operands_recovered_01.json",
        bytes: 23324,
        sha256: "83584ef89a735e82ec776b6841fafa8b2b2a63cc885265dc60b26a0cf20812a9",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_01.json",
        bytes: 1965927,
        sha256: "556048533b8819cbc479190d459e48b7fcdb4a5c430a034dc01e6dfb82d9b960",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_02.json",
        bytes: 1965313,
        sha256: "ec817a030dc816ac30e6b3e7718c9810f9329e3f4015807e1f6300a727415ab5",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_03.json",
        bytes: 2267707,
        sha256: "bb09e7b548fc74bd52bae283cc855d1ead0009df68686a0db64ea66f1bc30100",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_query_cases_04.json",
        bytes: 3295940,
        sha256: "164fd8cd84db26af0438c00d6cbd75bb014701c6f7a68159960a8ec3604f05be",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_comparison_supplement_query_cases.json",
        bytes: 929280,
        sha256: "8b8ee673236526ae844b249171c3035d5c4d6889f0c70d8e860725053426b93c",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_01.json",
        bytes: 2353918,
        sha256: "66e76c5147ef953c5463d06a8da54e81374e21962f939558c597d3f32d54a9c9",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_02.json",
        bytes: 2376286,
        sha256: "afb027b1fe66d96768c04cf1ae50ef995e9cbf1fc68c6441bdc4af91119b9695",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_03.json",
        bytes: 2430868,
        sha256: "0fd8fb0e8156c5fe7da1be4ad414e2788271e7d31a3c792fa292a059cc61628e",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/eval/fixtures/source_time_value_query_cases_04.json",
        bytes: 2580131,
        sha256: "a10de0b9ac332a79cb2f26242718b089cf5645c57f144868b6d40476b4ede4eb",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_original.json",
        bytes: 532348,
        sha256: "04897e1b5d024b7ef47b07caebde42a9445bbca3b99120678ff2f8474cd324c1",
    },
    FixtureBinding {
        path: "release-inputs-03/vendor/spareval/src/numeric/fixtures/source_variable_plan.json",
        bytes: 12954,
        sha256: "001e10de65e41964d10f9b88731a26dfe111bd83a66543f5c911a1fd8b4fb211",
    },
    FixtureBinding {
        path: "source-update-resources-08/SOURCE-UPDATE-OBSERVATION-PLAN-01.json",
        bytes: 159158,
        sha256: "9f9e246af0ad6b525049066dba4fa2b8c2a8d2a3b7317c321904bd19930a31b5",
    },
    FixtureBinding {
        path: "source-update-resources-08/SOURCE-UPDATE-OBSERVATION-PLAN-02.json",
        bytes: 16859,
        sha256: "c77a3857ac7c51e52ea1f6b64b8b19c35832d76941a3c6cb6673d3ab28c8cdfe",
    },
    FixtureBinding {
        path: "source-update-resources-08/source-update-local-authorized-01.ttl",
        bytes: 36,
        sha256: "eedb68b668c506c0005510644637c347c5d713f0d91fac32b79ec434ff032cee",
    },
    FixtureBinding {
        path: "source-update-additional-08/FROZEN-EXTERNAL-INPUTS-02.json",
        bytes: 6818,
        sha256: "c6211a76ded46fdfcf931e8161505849b5edddd174979726b23efbc9c86732ea",
    },
    FixtureBinding {
        path: "source-update-additional-08/CONTROLS-INPUTS-01.json",
        bytes: 3637,
        sha256: "a09b649babf96f24f3281cca0c7d75977574d056e053c88cf48e30417d698d82",
    },
    FixtureBinding {
        path: "source-update-additional-08/ORIGINAL-OBSERVATIONS-INDEPENDENT-REVIEW-01.json",
        bytes: 46772,
        sha256: "00e0b8c4c2d3cd9839fe13613d5abf0fd8e57492eb1d5f9e139c0d4310d10f6d",
    },
    FixtureBinding {
        path: "source-update-additional-08/external_blank_input_ascending_v2.json",
        bytes: 81559,
        sha256: "1926eaebbb387680173a3264daa78c6502664aca45451ecee3b8f5cf384d0914",
    },
    FixtureBinding {
        path: "source-update-additional-08/external_blank_input_descending_v2.json",
        bytes: 81561,
        sha256: "abaa7b9725665b99eab52d7681ab35899aa9ad69a5af44a53464376e2afea45d",
    },
    FixtureBinding {
        path: "source-update-additional-08/external_blank_input_nonmonotonic_v2.json",
        bytes: 81565,
        sha256: "d8e94b4fd8659581aa27c3cecd231d1b5d2e130944527cf1b9088746caed625f",
    },
    FixtureBinding {
        path: "source-update-additional-08/literal_false_optional.json",
        bytes: 23923,
        sha256: "d8a4f09a73639250c774d9f8b07dffedd82ccba959e7f218bbf7e68c07266ba3",
    },
    FixtureBinding {
        path: "source-update-additional-08/duplicate_union_update.json",
        bytes: 14502,
        sha256: "d1a8be0f8da1fd3895257afa8df2f05a9c73c6c3465369af47c8aa3e6a72d2a8",
    },
    FixtureBinding {
        path: "source-update-parser-08/FROZEN-EXTERNAL-INPUTS-01.json",
        bytes: 5935,
        sha256: "2877cd04178ebd1296af047fc01fe67c9b0d9672bbaa284cf21c2276203ddcfe",
    },
    FixtureBinding {
        path: "source-update-parser-08/PARSER-ORIGINAL-OBSERVATIONS-INDEPENDENT-REVIEW-01.json",
        bytes: 12495,
        sha256: "b26b41345679b3745451837dffd71401f96fa70206dfaaa18cdb25ffc676885a",
    },
    FixtureBinding {
        path: "source-update-parser-08/external_parser_blank_ascending_v1.json",
        bytes: 57299,
        sha256: "4e7eaa36982f39c1d481001cbbeba4d2cc1548c011316ca217962edf113831c6",
    },
    FixtureBinding {
        path: "source-update-parser-08/external_parser_blank_descending_v1.json",
        bytes: 57301,
        sha256: "4179eae4614640a30e09785124392b97587c234669a8ce8d95fa58e7e30815ab",
    },
    FixtureBinding {
        path: "source-storage-09/INPUTS-41.json",
        bytes: 10599,
        sha256: "7f0c9212bc29593e78527448c37100ffe3d1d6619ace0b9890cba7fcdfecec0b",
    },
    FixtureBinding {
        path: "source-storage-09/INPUTS-8.json",
        bytes: 3154,
        sha256: "1b82bb18ba45880230bad802e08e434dd3d1b5eb90f8fc0e018f23e9b8a9be30",
    },
    FixtureBinding {
        path: "source-storage-09/source_alias_update_roundtrip.json",
        bytes: 150740,
        sha256: "9befc9d8bdf0e6a9abeddb281bd318d33beef3f4bf6f2cc2c22cd0d6ac668ff1",
    },
    FixtureBinding {
        path: "source-storage-09/graph_selection_and_refusal.json",
        bytes: 34734,
        sha256: "65ca5cbbc6c042c70ac10b90c14dd7c059201999f3892b2f025b72ffc6f2e8d5",
    },
    FixtureBinding {
        path: "source-storage-09/generalized_source_refusal_boundary.json",
        bytes: 29045,
        sha256: "6f90bcbff255fb09e47a669cb1a79ee69880be44ff740b2108c3c9bcb07b7ee6",
    },
    FixtureBinding {
        path: "source-storage-09/source_diff_single_line_discrimination.json",
        bytes: 16209,
        sha256: "5180c594add82a32160fcea9d2f21446b6b85d88a39f0bbb87ffa83ad7a38850",
    },
    FixtureBinding {
        path: "source-storage-09/ORIGINAL-REVIEW.json",
        bytes: 206163,
        sha256: "29992a41bdb4213b502746b3bec632b2838cbcb3502bf48ae3b6630e59962012",
    },
    FixtureBinding {
        path: "source-constructor-composition-27/INPUTS-01.json",
        bytes: 27503,
        sha256: "5445c722c251d8794bdc6bd1c4c2cf9a69585fd94687190603eb5ed5a29c44d0",
    },
    FixtureBinding {
        path: "source-constructor-composition-27/FROZEN-CONSTRUCTION-PROJECTION-01.json",
        bytes: 2328,
        sha256: "55d0364836404466fcac7a9a1ac109eb572754c9742761d19272f0bc758cf638",
    },
    FixtureBinding {
        path: "source-constructor-composition-27/ORIGINAL-OBSERVATIONS-INDEPENDENT-REVIEW-01.json",
        bytes: 8405,
        sha256: "61e0d08d8ed425bd6c6d70090d8b074bb91bd86211d3e7b7ffb0f74fb94171ef",
    },
    FixtureBinding {
        path: "source-constructor-composition-27/constructor_composition_ascending_01.json",
        bytes: 80566,
        sha256: "1e599956cfa49801544dd61ee6af1fe24cbc4070ef5b540bdd8f1304d72939f6",
    },
    FixtureBinding {
        path: "source-constructor-composition-27/constructor_composition_descending_01.json",
        bytes: 80568,
        sha256: "368fcdb991967f430cc5c2b45dc6fd3f46e37da50a1308a6deaa4ee48239c8ea",
    },
    FixtureBinding {
        path: "source-default-blank-27/SOURCE-UPDATE-OBSERVATION-PLAN-01.json",
        bytes: 4702,
        sha256: "f8494a68439705e4cf4ed624b9c64358a7b1cb4fd261f426226a397dad975cb6",
    },
    FixtureBinding {
        path: "source-default-blank-27/ORIGINAL-RECEIVED-02.json",
        bytes: 8499,
        sha256: "7391a5b2ab5dc1aa5f200115a576865becc7e56e1f1675ca29790f7129288114",
    },
    FixtureBinding {
        path: "source-default-blank-27/ORIGINAL-OUTPUT-02.json",
        bytes: 26766,
        sha256: "9b40146a3177b951ffd403bf7b7386f5c158e3664be19f65697a627b200a3b4a",
    },
];

const MAX_ROOT_BYTES: usize = 4096;
const MAX_ROOT_COMPONENTS: usize = 128;
const MAX_FIXTURE_BYTES: u64 = 8 * 1024 * 1024;

fn refusal(code: &str, detail: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("{code}: {detail}"))
}

struct FixtureRoot {
    root: PathBuf,
    #[cfg(any(unix, windows))]
    directory: File,
}

impl FixtureRoot {
    fn from_environment() -> io::Result<Self> {
        let raw = std::env::var_os(ROOT_ENV);
        Self::from_path(raw.as_deref())
    }

    fn from_path(raw: Option<&OsStr>) -> io::Result<Self> {
        let raw = raw.ok_or_else(|| refusal("private_fixture_root_required", ROOT_ENV))?;
        let root = PathBuf::from(raw);
        if !root.is_absolute() {
            return Err(refusal(
                "private_fixture_root_invalid",
                "explicit absolute root required",
            ));
        }
        if root.as_os_str().len() > MAX_ROOT_BYTES
            || root.components().count() > MAX_ROOT_COMPONENTS
        {
            return Err(refusal(
                "private_fixture_root_budget",
                "root path exceeds fixed byte/component budget",
            ));
        }
        #[cfg(unix)]
        {
            let directory = absolute_directory(&root)?;
            Ok(Self { root, directory })
        }
        #[cfg(windows)]
        {
            let directory = windows_fixture::absolute_directory(&root)?;
            Ok(Self { root, directory })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = root;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "private_fixture_anchored_open_platform_unsupported",
            ))
        }
    }

    fn read(&self, id: &str) -> io::Result<Vec<u8>> {
        let binding = CATALOG
            .iter()
            .find(|b| b.path == id)
            .ok_or_else(|| refusal("private_fixture_unknown_id", id))?;
        self.read_binding(binding)
    }

    fn read_binding(&self, binding: &FixtureBinding) -> io::Result<Vec<u8>> {
        let relative = Path::new(binding.path);
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(refusal("private_fixture_path_invalid", binding.path));
        }
        if binding.bytes > MAX_FIXTURE_BYTES {
            return Err(refusal("private_fixture_size_budget", binding.path));
        }
        #[cfg(unix)]
        {
            self.check_root()?;
            let parent_path = relative
                .parent()
                .ok_or_else(|| refusal("private_fixture_path_invalid", binding.path))?;
            let name = relative
                .file_name()
                .ok_or_else(|| refusal("private_fixture_path_invalid", binding.path))?;
            // Every component is anchored to an already-open directory. Opening
            // a substituted FIFO cannot wait for its writer: all opens are
            // nonblocking, and fstat rejects nonregular payloads before reading.
            let parent = relative_directory(&self.directory, parent_path)?;
            let file = regular_file_at(&parent, name)?;
            let opened = file.metadata()?;
            if opened.len() != binding.bytes {
                return Err(refusal("private_fixture_size_mismatch", binding.path));
            }
            let limit = binding
                .bytes
                .checked_add(1)
                .ok_or_else(|| refusal("private_fixture_size_invalid", binding.path))?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(
                    usize::try_from(limit)
                        .map_err(|_| refusal("private_fixture_size_invalid", binding.path))?,
                )
                .map_err(|e| refusal("private_fixture_allocation_failed", e))?;
            (&file).take(limit).read_to_end(&mut bytes)?;
            if u64::try_from(bytes.len()).ok() != Some(binding.bytes) {
                return Err(refusal("private_fixture_size_mismatch", binding.path));
            }
            if private_fixture_sha256(&bytes) != binding.sha256 {
                return Err(refusal("private_fixture_digest_mismatch", binding.path));
            }
            self.check_root()?;
            let current_parent = relative_directory(&self.directory, parent_path)?;
            if !same_directory(&parent.metadata()?, &current_parent.metadata()?) {
                return Err(refusal("private_fixture_directory_replaced", binding.path));
            }
            let current_file = regular_file_at(&current_parent, name)?;
            if !same_file(&opened, &current_file.metadata()?)
                || !same_file(&opened, &file.metadata()?)
            {
                return Err(refusal("private_fixture_replaced", binding.path));
            }
            // This is the same owned Vec that was hashed. No pathname reopen is
            // used to obtain the consumer's data; the reopen above is metadata only.
            Ok(bytes)
        }
        #[cfg(windows)]
        {
            windows_fixture::read(&self.root, &self.directory, binding)
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "private_fixture_anchored_open_platform_unsupported",
            ))
        }
    }

    #[cfg(unix)]
    fn check_root(&self) -> io::Result<()> {
        let current = absolute_directory(&self.root)?;
        if !same_directory(&self.directory.metadata()?, &current.metadata()?) {
            return Err(refusal(
                "private_fixture_root_replaced",
                self.root.display(),
            ));
        }
        Ok(())
    }
}

// Kept in the shared test-only owner so every frozen-input consumer uses the
// same Windows acquisition, identity, and bounded-read contract.
#[cfg(windows)]
mod windows_fixture {
    use super::*;
    include!("private_test_fixtures_windows.rs");
}

#[cfg(unix)]
fn unix_name(name: &OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(name.as_bytes()).map_err(|e| refusal("private_fixture_path_invalid", e))
}

#[cfg(unix)]
fn directory_at(parent: &File, name: &OsStr) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let name = unix_name(name)?;
    // SAFETY: parent owns a live descriptor and name is a NUL-terminated C string.
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        let error = io::Error::last_os_error();
        let code = if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
            "private_fixture_directory_not_regular"
        } else {
            "private_fixture_directory_missing_or_unreadable"
        };
        return Err(refusal(code, error));
    }
    // SAFETY: openat returned a new descriptor transferred to this sole owner.
    let file = unsafe { File::from_raw_fd(descriptor) };
    if !file.metadata()?.is_dir() {
        return Err(refusal(
            "private_fixture_directory_not_regular",
            name.to_string_lossy(),
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn absolute_directory(path: &Path) -> io::Result<File> {
    use std::os::fd::FromRawFd;
    if !path.is_absolute() {
        return Err(refusal("private_fixture_root_invalid", path.display()));
    }
    // SAFETY: a valid static C string and flags; returned fd is immediately owned.
    let descriptor = unsafe {
        libc::open(
            c"/".as_ptr(),
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(refusal(
            "private_fixture_directory_missing_or_unreadable",
            io::Error::last_os_error(),
        ));
    }
    // SAFETY: open returned a new descriptor transferred to this sole owner.
    let mut directory = unsafe { File::from_raw_fd(descriptor) };
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(name) => directory = directory_at(&directory, name)?,
            _ => return Err(refusal("private_fixture_path_invalid", path.display())),
        }
    }
    Ok(directory)
}

#[cfg(unix)]
fn relative_directory(root: &File, path: &Path) -> io::Result<File> {
    let mut directory = root.try_clone()?;
    for part in path.components() {
        let Component::Normal(name) = part else {
            return Err(refusal("private_fixture_path_invalid", path.display()));
        };
        directory = directory_at(&directory, name)?;
    }
    Ok(directory)
}

#[cfg(unix)]
fn regular_file_at(parent: &File, name: &OsStr) -> io::Result<File> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let name = unix_name(name)?;
    // SAFETY: parent owns a live descriptor and name remains a valid C string.
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        let error = io::Error::last_os_error();
        let code = if error.raw_os_error() == Some(libc::ELOOP) {
            "private_fixture_not_regular"
        } else {
            "private_fixture_missing_or_unreadable"
        };
        return Err(refusal(code, error));
    }
    // SAFETY: openat returned a new descriptor transferred to this sole owner.
    let file = unsafe { File::from_raw_fd(descriptor) };
    if !file.metadata()?.is_file() {
        return Err(refusal(
            "private_fixture_not_regular",
            name.to_string_lossy(),
        ));
    }
    Ok(file)
}

#[cfg(unix)]
fn same_directory(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.is_dir() && b.is_dir() && a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(unix)]
fn same_file(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.file_type().is_file()
        && b.file_type().is_file()
        && a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
        && a.dev() == b.dev()
        && a.ino() == b.ino()
        && a.ctime() == b.ctime()
        && a.ctime_nsec() == b.ctime_nsec()
}

pub fn bytes(id: &str) -> Vec<u8> {
    FixtureRoot::from_environment()
        .and_then(|root| root.read(id))
        .unwrap_or_else(|e| panic!("private fixture {id} required: {e}"))
}

pub fn text(id: &str) -> String {
    String::from_utf8(bytes(id))
        .unwrap_or_else(|e| panic!("private_fixture_utf8_invalid: {id}: {e}"))
}

// Test call sites formerly embedded immutable text. Keep one verified owned
// string per explicit call site for the process lifetime; the first read must
// use the configured root and the frozen catalog. A failed read is not cached.
// This is test-only input storage, never an evaluator or a production loader.
#[allow(unused_macros)]
macro_rules! text_ref {
    ($id:literal) => {{
        static TEXT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        TEXT.get_or_init(|| crate::private_test_fixtures::text($id))
            .as_str()
    }};
}
#[allow(unused_imports)]
pub(crate) use text_ref;
