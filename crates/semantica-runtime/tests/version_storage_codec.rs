use checksum_native_candidate::{Encoder, Error, Limits};
use semantica_json::native_value::{NativeLimits, NativeNode as N, NativeValue};
use semantica_runtime::version_storage_codec as checksum_native_candidate;
use std::time::Duration;
fn value(nodes: Vec<N>) -> NativeValue {
    let root = nodes.len() - 1;
    NativeValue::new(
        nodes,
        root,
        NativeLimits {
            max_nodes: 1024,
            max_links: 4096,
            max_scalar_bytes: 65536,
            max_depth: 128,
            max_wire_bytes: 65536,
        },
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        visits: 10000,
        output_bytes: 65536,
        depth: 128,
        integer_digits: 65536,
        duration: Duration::from_secs(5),
    }
}
#[test]
fn ascii_expansion_is_charged_and_exact_boundary_is_retained() {
    let v = value(vec![N::Text(vec![0xe9])]);
    let mut l = limits();
    l.output_bytes = 8;
    assert_eq!(Encoder::new(&v, l).compute().unwrap().1, "\"\\u00e9\"");
    l.output_bytes = 7;
    assert_eq!(
        Encoder::new(&v, l).compute(),
        Err(Error::Refused("output_bytes"))
    );
}
#[test]
fn repeated_references_cannot_bypass_work_or_output_limits() {
    let mut nodes = vec![N::Integer("1".into())];
    for i in 0..64 {
        nodes.push(N::List(vec![i, i]));
    }
    let v = value(nodes);
    let mut l = limits();
    l.visits = 100;
    assert_eq!(Encoder::new(&v, l).compute(), Err(Error::Refused("visits")));
    l.visits = 10000;
    l.output_bytes = 64;
    assert_eq!(
        Encoder::new(&v, l).compute(),
        Err(Error::Refused("output_bytes"))
    );
}
#[test]
fn depth_and_expired_execution_are_resource_refusals() {
    let v = value(vec![
        N::Integer("1".into()),
        N::List(vec![0]),
        N::List(vec![1]),
    ]);
    let mut l = limits();
    l.depth = 1;
    assert_eq!(Encoder::new(&v, l).compute(), Err(Error::Refused("depth")));
    l = limits();
    l.duration = Duration::ZERO;
    assert_eq!(Encoder::new(&v, l).compute(), Err(Error::Refused("time")));
}
#[test]
fn source_integer_failure_is_distinct_from_local_resource_refusal() {
    let v = value(vec![N::Integer("9".repeat(4301))]);
    let result = Encoder::new(&v, limits()).compute();
    assert!(matches!(
        result,
        Err(Error::Source {
            class: "builtins.ValueError",
            ..
        })
    ));
    let v = value(vec![
        N::Integer("1".into()),
        N::Text(vec![97]),
        N::Integer("9".repeat(5000)),
        N::Text(vec![98]),
        N::Dict(vec![(0, 1), (2, 3)]),
    ]);
    let mut l = limits();
    l.integer_digits = 4500;
    assert_eq!(
        Encoder::new(&v, l).compute(),
        Err(Error::Refused("numeric_digits"))
    );
}
#[test]
fn verification_preserves_original_falsey_short_circuit() {
    let v = value(vec![
        N::Text("checksum".chars().map(u32::from).collect()),
        N::Bool(false),
        N::Text(vec![97]),
        N::Set(vec![]),
        N::Dict(vec![(0, 1), (2, 3)]),
    ]);
    assert!(!Encoder::new(&v, limits()).verify().unwrap());
    assert!(matches!(
        Encoder::new(&v, limits()).compute(),
        Err(Error::Source {
            class: "builtins.TypeError",
            ..
        })
    ));
}

fn parse_limits() -> semantica_json::original_json::ParseLimits {
    semantica_json::original_json::ParseLimits {
        decode: semantica_json::original_json_bytes::DecodeLimits {
            max_input_bytes: 65536,
            max_codepoints: 65536,
        },
        native: NativeLimits {
            max_nodes: 1024,
            max_links: 4096,
            max_scalar_bytes: 65536,
            max_depth: 128,
            max_wire_bytes: 65536,
        },
    }
}
#[test]
fn storage_copy_enforces_reader_budget_without_partial_result() {
    let v = value(vec![
        N::Integer("1".into()),
        N::Integer("2".into()),
        N::Tuple(vec![0, 1]),
    ]);
    let before = v.to_envelope(parse_limits().native).unwrap();
    let mut parse = parse_limits();
    parse.native.max_nodes = 2;
    assert!(matches!(
        Encoder::new(&v, limits()).storage_copy(parse),
        Err(Error::Refused("storage_parse_resource"))
    ));
    assert_eq!(v.to_envelope(parse_limits().native).unwrap(), before);
}
#[test]
fn storage_default_spaces_are_counted_independently_from_checksum_compaction() {
    let v = value(vec![
        N::Integer("1".into()),
        N::Integer("2".into()),
        N::Tuple(vec![0, 1]),
    ]);
    let mut l = limits();
    l.output_bytes = 5;
    assert_eq!(Encoder::new(&v, l).compute().unwrap().1, "[1,2]");
    assert!(matches!(
        Encoder::new(&v, l).storage_copy(parse_limits()),
        Err(Error::Refused("output_bytes"))
    ));
    l.output_bytes = 6;
    let (text, copied) = Encoder::new(&v, l).storage_copy(parse_limits()).unwrap();
    assert_eq!(text, "[1, 2]");
    assert!(matches!(&copied.nodes()[copied.root()], N::List(_)));
}
#[test]
fn storage_text_key_collision_is_resolved_by_original_reader() {
    let v = value(vec![
        N::Integer("1".into()),
        N::Text(vec![97]),
        N::Text(vec![49]),
        N::Text(vec![98]),
        N::Dict(vec![(0, 1), (2, 3)]),
    ]);
    let (text, copied) = Encoder::new(&v, limits())
        .storage_copy(parse_limits())
        .unwrap();
    assert_eq!(text, "{\"1\": \"a\", \"1\": \"b\"}");
    assert_eq!(
        Encoder::new(&copied, limits()).compute().unwrap().1,
        "{\"1\":\"b\"}"
    );
}
