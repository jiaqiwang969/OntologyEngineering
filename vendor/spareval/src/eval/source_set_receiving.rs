//! Receives original set(generator) traces through the production pure set.
//! Inputs and expected records are mechanical frozen projections. The driver
//! supplies controlled caller hash/identity/equality events; it implements no
//! probing, table growth, duplicate handling, or iteration algorithm.
use super::source_set::{SourceSet, SourceSetError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

const FIXTURE: &str = include_str!("fixtures/source_context_set_cases.json");

fn fixture() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "8c6ac659d5b1c79bccbb8ce646a6f2f108890252dc1ac5179b943213ca153977"
    );
    let root: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        root["source_record_sha256"],
        "9069b3597b3867008827f74397d9bacdab7eb43dcf941f9d1d715d314d45eeb4"
    );
    root
}

struct Capture {
    full: Option<Vec<Value>>,
    count: usize,
    first: Vec<Value>,
    last: Vec<Value>,
    digest: Sha256,
}
impl Capture {
    fn new(full: bool) -> Self {
        Self {
            full: full.then(Vec::new),
            count: 0,
            first: Vec::new(),
            last: Vec::new(),
            digest: Sha256::new(),
        }
    }
    fn push(&mut self, value: Value) {
        self.count += 1;
        // Trace elements are arrays/strings: canonical Python JSON has the
        // same UTF-8 spelling here, with no object-key ordering dependency.
        self.digest.update(serde_json::to_vec(&value).unwrap());
        self.digest.update(b"\n");
        if self.first.len() < 8 {
            self.first.push(value.clone());
        }
        self.last.push(value.clone());
        if self.last.len() > 8 {
            self.last.remove(0);
        }
        if let Some(full) = &mut self.full {
            full.push(value);
        }
    }
    fn finish(self) -> Value {
        json!({"count":self.count, "first":self.first,"last":self.last,"full":self.full,
            "sha256":format!("{:x}",self.digest.finalize())})
    }
}

struct Row {
    tag: String,
    hash: i64,
    equal: String,
    payload: String,
}
#[derive(Debug)]
struct Marker;

fn receive(id: &str) {
    let root = fixture();
    let case = root["set_cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"]["id"] == id)
        .unwrap();
    let input = &case["input"];
    let expected = &case["expected"];
    let full = input["capture"] != "digest_only";
    let trace = RefCell::new(Capture::new(full));
    let mut objects = BTreeMap::<String, Rc<Row>>::new();
    let mut order = Vec::new();
    if let Some(count) = input["range"]["count"].as_u64() {
        let start: i64 = input["range"]["hash_start"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let stride: i64 = input["range"]["hash_stride"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        for i in 0..count {
            let tag = format!("r{i}");
            objects.insert(
                tag.clone(),
                Rc::new(Row {
                    tag: tag.clone(),
                    hash: start + (i as i64) * stride,
                    equal: tag.clone(),
                    payload: format!("payload-{i}"),
                }),
            );
            order.push(tag);
        }
    } else {
        for input in input["items"].as_array().unwrap() {
            let tag = input["tag"].as_str().unwrap().to_owned();
            objects.insert(
                tag.clone(),
                Rc::new(Row {
                    tag,
                    hash: input["hash"].as_str().unwrap().parse().unwrap(),
                    equal: input["equal"].as_str().unwrap().to_owned(),
                    payload: input["payload"].as_str().unwrap().to_owned(),
                }),
            );
        }
        order = input["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tag| tag.as_str().unwrap().to_owned())
            .collect();
    }
    let fault = &input["fault"];
    let marker = Rc::new(Marker);
    let mut set = SourceSet::new();
    let result = {
        let mut same = Rc::ptr_eq;
        let mut equal = |left: &Rc<Row>, right: &Rc<Row>| -> Result<bool, Rc<Marker>> {
            trace.borrow_mut().push(json!(["eq", left.tag, right.tag]));
            if fault["kind"] == "equal" && fault["pair"] == json!([left.tag, right.tag]) {
                trace
                    .borrow_mut()
                    .push(json!(["raise", "equal", left.tag, right.tag]));
                return Err(Rc::clone(&marker));
            }
            let result = left.equal == right.equal;
            trace
                .borrow_mut()
                .push(json!(["eq_return", left.tag, right.tag, result]));
            Ok(result)
        };
        let mut cancel = || Ok(());
        (|| -> Result<(), SourceSetError<Rc<Marker>>> {
            for (index, tag) in order.iter().enumerate() {
                if fault["kind"] == "iterator" && fault["before_yield"] == index {
                    trace.borrow_mut().push(json!(["raise", "iterator", index]));
                    return Err(SourceSetError::Callback(Rc::clone(&marker)));
                }
                trace.borrow_mut().push(json!(["yield", tag]));
                let row = Rc::clone(&objects[tag]);
                trace
                    .borrow_mut()
                    .push(json!(["hash", row.tag, row.hash.to_string()]));
                if fault["kind"] == "hash" && fault["tag"] == row.tag {
                    trace.borrow_mut().push(json!(["raise", "hash", row.tag]));
                    return Err(SourceSetError::Callback(Rc::clone(&marker)));
                }
                let hash = row.hash;
                set.insert_hashed(row, hash, &mut same, &mut equal, &mut cancel)?;
            }
            if fault["kind"] == "iterator" && fault["before_yield"] == order.len() {
                trace
                    .borrow_mut()
                    .push(json!(["raise", "iterator", order.len()]));
                return Err(SourceSetError::Callback(Rc::clone(&marker)));
            }
            trace.borrow_mut().push(json!(["exhausted"]));
            Ok(())
        })()
    };
    assert_eq!(
        trace.into_inner().finish(),
        expected["trace"],
        "{id}: original event schedule"
    );
    if expected["status"] == "original_marker" {
        let Err(SourceSetError::Callback(actual)) = result else {
            panic!("{id}: expected original marker")
        };
        assert!(Rc::ptr_eq(&actual, &marker), "{id}: exception identity");
        // Failure does not expose the private partial table to the consumer.
        return;
    }
    result.unwrap();
    let mut permutation = Capture::new(full);
    let mut representatives = full.then(Vec::new);
    for row in set.iter() {
        permutation.push(json!(row.tag));
        assert!(
            Rc::ptr_eq(row, &objects[&row.tag]),
            "{id}: original representative object"
        );
        if let Some(representatives) = &mut representatives {
            representatives.push(json!({"tag":row.tag,"equal":row.equal,"payload":row.payload}));
        }
    }
    assert_eq!(
        set.len(),
        expected["iteration_tags"]["count"].as_u64().unwrap() as usize
    );
    assert_eq!(
        permutation.finish(),
        expected["iteration_tags"],
        "{id}: original slot order"
    );
    assert_eq!(
        json!(representatives),
        expected["representatives"],
        "{id}: first full representative"
    );
}

#[test]
fn source_set_original_empty() {
    receive("SS01-empty");
}

#[test]
fn source_set_original_singleton() {
    receive("SS02-singleton");
}

#[test]
fn source_set_original_before_first_resize() {
    receive("SS03-resize-4");
}

#[test]
fn source_set_original_first_resize() {
    receive("SS03-resize-5");
}

#[test]
fn source_set_original_before_second_resize() {
    receive("SS03-resize-18");
}

#[test]
fn source_set_original_second_resize() {
    receive("SS03-resize-19");
}

#[test]
fn source_set_original_before_third_resize() {
    receive("SS03-resize-76");
}

#[test]
fn source_set_original_third_resize() {
    receive("SS03-resize-77");
}

#[test]
fn source_set_original_same_hash_collisions() {
    receive("SS04-samehash-collision");
}

#[test]
fn source_set_original_upper_hash_bits() {
    receive("SS05-upperbits");
}

#[test]
fn source_set_original_signed_hash_normalization() {
    receive("SS06-signed-normalization");
}

#[test]
fn source_set_original_equal_retains_first_object() {
    receive("SS07-equal-first");
}

#[test]
fn source_set_original_same_object_still_hashed() {
    receive("SS08-same-instance");
}

#[test]
fn source_set_original_late_hash_marker() {
    receive("SS09-late-hash-marker");
}

#[test]
fn source_set_original_equality_marker() {
    receive("SS10-equality-marker");
}

#[test]
fn source_set_original_iterator_marker() {
    receive("SS11-iterator-marker");
}

#[test]
fn source_set_original_large_growth_threshold() {
    receive("SS12-large-growth");
}
