//! Actual CPython mutable-set observations, frozen before this implementation.
//! The driver controls hashes/equality/faults and invokes production operations;
//! it contains no probing, resizing, dummy-slot, or iteration algorithm.
use super::source_set::{SourceSet, SourceSetError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

const FIXTURE: &str = include_str!("fixtures/source_mutable_set_cases.json");

#[derive(Default)]
struct Capture {
    count: usize,
    first: Vec<Value>,
    last: Vec<Value>,
    full: Vec<Value>,
    digest: Sha256,
}
impl Capture {
    fn push(&mut self, value: Value) {
        self.count += 1;
        self.digest.update(serde_json::to_vec(&value).unwrap());
        self.digest.update(b"\n");
        if self.first.len() < 8 {
            self.first.push(value.clone());
        }
        self.last.push(value.clone());
        if self.last.len() > 8 {
            self.last.remove(0);
        }
        if self.count <= 512 {
            self.full.push(value);
        } else {
            self.full.clear();
        }
    }
    fn finish(self) -> Value {
        json!({"count":self.count,"first":self.first,"last":self.last,
            "full": if self.count <= 512 {Some(self.full)} else {None},
            "sha256":format!("{:x}",self.digest.finalize())})
    }
}
struct Key {
    id: u64,
    tag: String,
    payload: String,
    hash: i64,
    equal: String,
}
#[derive(Debug)]
struct Marker;

fn receive(id: &str) {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "c16de1cbf97c900d4335d3160bd92867b2f77cce9cc4efa840f55f5b4e9b16ec"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["frozen_sha256"],
        "7c5ac13ac44db212def781f790a87fa57eb23ecfa48d689eca00986770aea554"
    );
    let case = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["input"]["id"] == id)
        .unwrap();
    let input = &case["input"];
    let start: i64 = input["keys"]["hash_start"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let stride: i64 = input["keys"]["hash_stride"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let keys: Vec<_> = (0..input["keys"]["count"].as_u64().unwrap())
        .map(|i| {
            Rc::new(Key {
                id: i,
                tag: format!("k{i}"),
                payload: format!("payload-{i}"),
                hash: start + i as i64 * stride,
                equal: input["equal_groups"][i.to_string()]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| i.to_string()),
            })
        })
        .collect();
    let mut sets = BTreeMap::<String, SourceSet<Rc<Key>>>::new();
    let marker = Rc::new(Marker);
    for (index, action) in input["actions"].as_array().unwrap().iter().enumerate() {
        let trace = RefCell::new(Capture::default());
        let target = action["target"].as_str().unwrap().to_owned();
        let fault = &action["fault"];
        let hash = |key: &Rc<Key>| {
            trace.borrow_mut().push(json!(["hash", key.tag]));
            if fault["hash"].as_u64() == Some(key.id) {
                trace.borrow_mut().push(json!(["raise", "hash", key.tag]));
                Err(SourceSetError::Callback(Rc::clone(&marker)))
            } else {
                Ok(key.hash)
            }
        };
        let mut equal = |left: &Rc<Key>, right: &Rc<Key>| {
            trace.borrow_mut().push(json!(["eq", left.tag, right.tag]));
            if fault["equal"] == json!([left.id, right.id]) {
                trace
                    .borrow_mut()
                    .push(json!(["raise", "equal", left.tag, right.tag]));
                Err(Rc::clone(&marker))
            } else {
                Ok(left.equal == right.equal)
            }
        };
        let mut same = Rc::ptr_eq;
        let mut cancel = || Ok(());
        let mut current = sets.remove(&target).unwrap_or_else(SourceSet::new);
        let result = (|| -> Result<Value, SourceSetError<Rc<Marker>>> {
            match action["op"].as_str().unwrap() {
                "add_many" => {
                    for key in action["keys"].as_array().unwrap() {
                        let key = &keys[key.as_u64().unwrap() as usize];
                        current.insert_hashed(
                            Rc::clone(key),
                            hash(key)?,
                            &mut same,
                            &mut equal,
                            &mut cancel,
                        )?;
                    }
                    Ok(Value::Null)
                }
                "discard_many" => {
                    let mut found = Vec::new();
                    for key in action["keys"].as_array().unwrap() {
                        let key = &keys[key.as_u64().unwrap() as usize];
                        found.push(current.discard_hashed(
                            key,
                            hash(key)?,
                            &mut same,
                            &mut equal,
                            &mut cancel,
                        )?);
                    }
                    Ok(json!(found))
                }
                "contains" => {
                    let key = &keys[action["key"].as_u64().unwrap() as usize];
                    Ok(json!(current.contains_hashed(
                        key,
                        hash(key)?,
                        &mut same,
                        &mut equal,
                        &mut cancel
                    )?))
                }
                "copy" => {
                    let source = action["source"].as_str().unwrap();
                    let original = if source == target {
                        &current
                    } else {
                        &sets[source]
                    };
                    current = original.copy_hashed(&mut same, &mut equal, &mut cancel)?;
                    Ok(Value::Null)
                }
                "update" => {
                    let source = action["source"].as_str().unwrap();
                    // Python's identity shortcut belongs to the caller; Rust
                    // mutable borrowing cannot alias this production argument.
                    if source != target {
                        current.merge_hashed(&sets[source], &mut same, &mut equal, &mut cancel)?;
                    }
                    Ok(Value::Null)
                }
                "clear" => {
                    current.clear();
                    Ok(Value::Null)
                }
                other => panic!("unknown original action {other}"),
            }
        })();
        sets.insert(target, current);
        let (value, error) = match result {
            Ok(value) => (value, Value::Null),
            Err(SourceSetError::Callback(actual)) => {
                assert!(Rc::ptr_eq(&actual, &marker));
                (
                    Value::Null,
                    json!({"kind":"controlled_callback","same_marker":true}),
                )
            }
            Err(other) => panic!("unexpected production error {id}/{index}: {other:?}"),
        };
        let state: BTreeMap<_, _> = sets
            .iter()
            .map(|(name, set)| {
                let mut rows = Capture::default();
                for key in set.iter() {
                    rows.push(json!([key.tag, key.payload]));
                }
                assert_eq!(rows.count, set.len());
                (name, rows.finish())
            })
            .collect();
        let actual =
            json!({"result":value,"error":error,"trace":trace.into_inner().finish(),"sets":state});
        assert_eq!(
            actual, case["expected"][index],
            "original {id} action {index}: {action}"
        );
    }
}

macro_rules! receiving {
    ($($name:ident => $id:literal),+ $(,)?) => { $(#[test] fn $name(){receive($id);})+ };
}
receiving! {
    source_mutable_set_ms01_delete_reinsert => "MS01",
    source_mutable_set_ms02_growth_and_dummy_copy => "MS02",
    source_mutable_set_ms03_signed_hashes => "MS03",
    source_mutable_set_ms04_representative_after_deletion => "MS04",
    source_mutable_set_ms05_empty_with_dummies => "MS05",
    source_mutable_set_ms06_mutated_union => "MS06",
    source_mutable_set_ms07_hash_failure_partial_progress => "MS07",
    source_mutable_set_ms08_equality_failure_partial_progress => "MS08",
    source_mutable_set_ms09_upper_hash_bits => "MS09",
    source_mutable_set_ms10_copy_shrink => "MS10",
    source_mutable_set_ms11_negative_one_and_small_rebuild => "MS11",
    source_mutable_set_ms12_large_growth_mutation => "MS12",
}
