use semantica_json::native_value::{NativeError, NativeLimits, NativeNode as N, NativeValue};
use serde_json::{Value, json};
fn limits() -> NativeLimits {
    NativeLimits {
        max_nodes: 10000,
        max_links: 20000,
        max_scalar_bytes: 1_000_000,
        max_depth: 256,
        max_wire_bytes: 4_000_000,
    }
}
fn original(value: &Value, nodes: &mut Vec<N>) -> usize {
    let node = match value["type"].as_str().unwrap() {
        "none" => N::None,
        "bool" => N::Bool(value["value"].as_bool().unwrap()),
        "int" => N::Integer(value["decimal"].as_str().unwrap().into()),
        "float" => N::FloatBits(
            u64::from_str_radix(value["ieee754_be_hex"].as_str().unwrap(), 16).unwrap(),
        ),
        "str" => N::Text(
            value["codepoints"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u32)
                .collect(),
        ),
        "dict" => N::Dict(
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| (original(&p[0], nodes), original(&p[1], nodes)))
                .collect(),
        ),
        kind => {
            let ids = value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| original(v, nodes))
                .collect();
            match kind {
                "list" => N::List(ids),
                "tuple" => N::Tuple(ids),
                "set" => N::Set(ids),
                "frozenset" => N::FrozenSet(ids),
                _ => panic!("{kind}"),
            }
        }
    };
    let id = nodes.len();
    nodes.push(node);
    id
}
fn assert_original(value: &Value, native: &NativeValue, id: usize) {
    match (&native.nodes()[id], value["type"].as_str().unwrap()) {
        (N::None, "none") => {}
        (N::Bool(v), "bool") => assert_eq!(json!(v), value["value"]),
        (N::Integer(v), "int") => assert_eq!(json!(v), value["decimal"]),
        (N::FloatBits(v), "float") => assert_eq!(
            format!("{v:016x}"),
            value["ieee754_be_hex"].as_str().unwrap()
        ),
        (N::Text(v), "str") => assert_eq!(json!(v), value["codepoints"]),
        (N::Dict(pairs), "dict") => {
            let expected = value["items"].as_array().unwrap();
            assert_eq!(pairs.len(), expected.len());
            for ((k, v), p) in pairs.iter().zip(expected) {
                assert_original(&p[0], native, *k);
                assert_original(&p[1], native, *v);
            }
        }
        (node, kind) => {
            let ids = match (node, kind) {
                (N::List(ids), "list")
                | (N::Tuple(ids), "tuple")
                | (N::Set(ids), "set")
                | (N::FrozenSet(ids), "frozenset") => ids,
                _ => panic!("type mismatch {node:?} {kind}"),
            };
            let expected = value["items"].as_array().unwrap();
            assert_eq!(ids.len(), expected.len());
            for (id, v) in ids.iter().zip(expected) {
                assert_original(v, native, *id);
            }
        }
    }
}
#[test]
fn six_real_original_import_states_keep_every_type_key_codepoint_and_float_bit() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/original_import_native_values.json")).unwrap();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        let mut nodes = vec![];
        let root = original(&row["typed_state"], &mut nodes);
        let native = NativeValue::new(nodes, root, limits()).unwrap();
        let bytes = native.encode(limits()).unwrap();
        let decoded = NativeValue::decode(&bytes, limits()).unwrap();
        assert_eq!(decoded.encode(limits()).unwrap(), bytes);
        assert_original(&row["typed_state"], &decoded, decoded.root());
    }
}
#[test]
fn plain_json_bridge_never_coerces_special_types_or_literal_tag_objects() {
    for node in [
        N::FloatBits(f64::NAN.to_bits()),
        N::FloatBits(f64::INFINITY.to_bits()),
        N::FloatBits(f64::NEG_INFINITY.to_bits()),
        N::Text(vec![0xd800]),
        N::Tuple(vec![]),
        N::Set(vec![]),
        N::FrozenSet(vec![]),
    ] {
        let value = NativeValue::new(vec![node], 0, limits()).unwrap();
        assert_eq!(
            value.to_plain_json(limits()).unwrap_err(),
            NativeError::LossyJson
        );
    }
    let nodes = vec![
        N::None,
        N::Integer("1".into()),
        N::Text("null".chars().map(u32::from).collect()),
        N::Integer("2".into()),
        N::Dict(vec![(0, 1), (2, 3)]),
    ];
    let value = NativeValue::new(nodes, 4, limits()).unwrap();
    let restored = NativeValue::decode(&value.encode(limits()).unwrap(), limits()).unwrap();
    assert_eq!(
        restored.to_plain_json(limits()).unwrap_err(),
        NativeError::LossyJson
    );
    let literal = json!({"kind":"float64","bits":"7ff8000000000000","nested":{"schema":"semantica.native-value/1","nodes":[]}});
    let native = NativeValue::from_plain_json(&literal, limits()).unwrap();
    assert_eq!(native.to_plain_json(limits()).unwrap(), literal);
    let huge: Value = serde_json::from_str(&format!("1{}", "0".repeat(400))).unwrap();
    let native = NativeValue::from_plain_json(&huge, limits()).unwrap();
    assert_eq!(native.to_plain_json(limits()).unwrap(), huge);
    let minus_zero =
        NativeValue::new(vec![N::FloatBits((-0.0f64).to_bits())], 0, limits()).unwrap();
    assert_eq!(
        minus_zero
            .to_plain_json(limits())
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
}
#[test]
fn invalid_structure_and_all_caller_limits_are_explicit_refusals() {
    for (nodes, root, error) in [
        (vec![N::List(vec![0])], 0, NativeError::Reference),
        (vec![N::None, N::None], 0, NativeError::Unreachable),
        (vec![N::Text(vec![0x110000])], 0, NativeError::Codepoint),
        (vec![N::Integer("01".into())], 0, NativeError::Integer),
        (vec![N::Integer("-0".into())], 0, NativeError::Integer),
        (
            vec![N::List(vec![]), N::None, N::Dict(vec![(0, 1)])],
            2,
            NativeError::UnhashableKey,
        ),
    ] {
        assert_eq!(NativeValue::new(nodes, root, limits()).unwrap_err(), error);
    }
    let native = NativeValue::new(vec![N::Text(vec![65]), N::List(vec![0])], 1, limits()).unwrap();
    let base = limits();
    for (limit, error) in [
        (
            NativeLimits {
                max_nodes: 1,
                ..base
            },
            NativeError::NodeLimit,
        ),
        (
            NativeLimits {
                max_links: 0,
                ..base
            },
            NativeError::LinkLimit,
        ),
        (
            NativeLimits {
                max_scalar_bytes: 3,
                ..base
            },
            NativeError::ScalarLimit,
        ),
        (
            NativeLimits {
                max_depth: 0,
                ..base
            },
            NativeError::DepthLimit,
        ),
        (
            NativeLimits {
                max_wire_bytes: 1,
                ..base
            },
            NativeError::WireLimit,
        ),
    ] {
        assert_eq!(native.encode(limit).unwrap_err(), error);
    }
    let bytes =
        br#"{"schema":"semantica.native-value/1","root":0,"root":0,"nodes":[{"kind":"none"}]}"#;
    assert_eq!(
        NativeValue::decode(bytes, limits()).unwrap_err(),
        NativeError::MalformedJson
    );
    let mut wire = native.to_envelope(limits()).unwrap();
    wire["nodes"][0]["extra"] = json!("ignored?");
    assert_eq!(
        NativeValue::from_envelope(&wire, limits()).unwrap_err(),
        NativeError::Shape
    );
}
#[test]
fn deep_native_containers_have_flat_wire_and_shared_expansion_is_bounded() {
    let mut nodes = vec![N::None];
    for i in 0..200 {
        nodes.push(N::List(vec![i]));
    }
    let native = NativeValue::new(nodes, 200, limits()).unwrap();
    let wire = native.encode(limits()).unwrap();
    assert_eq!(
        NativeValue::decode(&wire, limits()).unwrap().nodes().len(),
        201
    );
    assert_eq!(
        native.to_plain_json(limits()).unwrap_err(),
        NativeError::DepthLimit
    );
    let mut nodes = vec![N::None];
    for i in 0..20 {
        nodes.push(N::List(vec![i, i]));
    }
    let native = NativeValue::new(nodes, 20, limits()).unwrap();
    assert_eq!(
        native.to_plain_json(limits()).unwrap_err(),
        NativeError::NodeLimit
    );
}

#[test]
fn shared_large_text_refuses_expanded_wire_before_materializing_copies() {
    let native = NativeValue::new(
        vec![N::Text(vec![65; 100_000]), N::List(vec![0; 10_000])],
        1,
        limits(),
    )
    .unwrap();
    assert_eq!(
        native.to_plain_json(limits()).unwrap_err(),
        NativeError::NodeLimit
    );
    let large = NativeLimits {
        max_nodes: 100_000,
        ..limits()
    };
    assert_eq!(
        native.to_plain_json(large).unwrap_err(),
        NativeError::WireLimit
    );
}

#[test]
fn strict_json_remains_strict_while_explicit_native_wire_retains_values() {
    assert!(semantica_json::strict_json(b"NaN").is_err());
    assert!(semantica_json::strict_json(br#""\ud800""#).is_err());
    assert!(semantica_json::strict_json(br#"{"x":1,"x":2}"#).is_err());
    let value = NativeValue::new(
        vec![
            N::FloatBits(f64::NAN.to_bits()),
            N::Text(vec![0xd800]),
            N::List(vec![0, 1]),
        ],
        2,
        limits(),
    )
    .unwrap();
    let wire = value.encode(limits()).unwrap();
    assert!(semantica_json::strict_json(&wire).is_ok());
    assert!(NativeValue::decode(&wire, limits()).is_ok());
}
