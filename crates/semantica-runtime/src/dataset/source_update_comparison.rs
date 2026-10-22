//! Test-only original-observer comparison. Execution never receives these maps.
//! One frontier of bijections spans every action, source set and `new` boundary.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Bijection {
    expected_to_actual: BTreeMap<String, String>,
    actual_to_expected: BTreeMap<String, String>,
}
impl Bijection {
    fn bind(&mut self, expected: &str, actual: &str) -> bool {
        if self
            .expected_to_actual
            .get(expected)
            .is_some_and(|v| v != actual)
            || self
                .actual_to_expected
                .get(actual)
                .is_some_and(|v| v != expected)
        {
            return false;
        }
        self.expected_to_actual
            .insert(expected.into(), actual.into());
        self.actual_to_expected
            .insert(actual.into(), expected.into());
        true
    }
}

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub search_steps: usize,
    pub mappings: usize,
    pub depth: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            search_steps: 1_000_000,
            mappings: 4096,
            depth: 128,
        }
    }
}
struct Search {
    limits: Limits,
    steps: usize,
}
impl Search {
    fn step(&mut self, depth: usize) -> Result<(), String> {
        self.steps = self
            .steps
            .checked_add(1)
            .ok_or("comparison_search_overflow")?;
        if self.steps > self.limits.search_steps {
            return Err("comparison_search_budget".into());
        }
        if depth > self.limits.depth {
            return Err("comparison_depth_budget".into());
        }
        Ok(())
    }
    fn unique(&self, maps: Vec<Bijection>) -> Result<Vec<Bijection>, String> {
        let unique: BTreeSet<_> = maps.into_iter().collect();
        if unique.len() > self.limits.mappings {
            return Err("comparison_mapping_budget".into());
        }
        Ok(unique.into_iter().collect())
    }
}

pub(super) use super::super::source_observation::{observer_key, sort_observation};

fn check_observation_sort(row: &Value) -> Result<(), String> {
    for (field, ascii) in [("quads", false), ("contexts", true)] {
        let Some(values) = row
            .get("state")
            .and_then(|v| v.get(field))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let keys = values
            .iter()
            .map(|v| observer_key(v, ascii))
            .collect::<Result<Vec<_>, _>>()?;
        if keys.windows(2).any(|p| p[0] > p[1]) {
            return Err(format!("original_observer_order:/state/{field}"));
        }
    }
    Ok(())
}
fn pointer(parent: &str, key: &str) -> String {
    format!("{parent}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn match_value(
    expected: &Value,
    actual: &Value,
    map: Bijection,
    path: &str,
    depth: usize,
    search: &mut Search,
) -> Result<Vec<Bijection>, String> {
    search.step(depth)?;
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            if e.len() != a.len() || e.keys().any(|k| !a.contains_key(k)) {
                return Ok(vec![]);
            }
            let blank = e.get("term_type").and_then(Value::as_str) == Some("blank_node");
            let mut map = map;
            if blank {
                if a.get("term_type").and_then(Value::as_str) != Some("blank_node") {
                    return Ok(vec![]);
                }
                let (Some(e), Some(a)) = (
                    e.get("value").and_then(Value::as_str),
                    a.get("value").and_then(Value::as_str),
                ) else {
                    return Ok(vec![]);
                };
                if !map.bind(e, a) {
                    return Ok(vec![]);
                }
            }
            let mut candidates = vec![map];
            // Stable traversal is a comparison choice; all viable choices are
            // retained until later observations resolve a symmetric graph.
            let keys: BTreeSet<_> = e.keys().collect();
            for key in keys {
                if blank && key == "value" {
                    continue;
                }
                let mut next = vec![];
                for candidate in candidates {
                    next.extend(match_value(
                        &e[key],
                        &a[key],
                        candidate,
                        &pointer(path, key),
                        depth + 1,
                        search,
                    )?);
                    if next.len() > search.limits.mappings {
                        next = search.unique(next)?;
                    }
                }
                candidates = search.unique(next)?;
                if candidates.is_empty() {
                    break;
                }
            }
            Ok(candidates)
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                return Ok(vec![]);
            }
            if path == "/state/quads" || path == "/state/contexts" {
                match_unordered(
                    e,
                    a,
                    0,
                    &mut vec![false; a.len()],
                    map,
                    path,
                    depth + 1,
                    search,
                )
            } else {
                let mut candidates = vec![map];
                for (i, (e, a)) in e.iter().zip(a).enumerate() {
                    let mut next = vec![];
                    for candidate in candidates {
                        next.extend(match_value(
                            e,
                            a,
                            candidate,
                            &pointer(path, &i.to_string()),
                            depth + 1,
                            search,
                        )?);
                        if next.len() > search.limits.mappings {
                            next = search.unique(next)?;
                        }
                    }
                    candidates = search.unique(next)?;
                    if candidates.is_empty() {
                        break;
                    }
                }
                Ok(candidates)
            }
        }
        _ => Ok(if expected == actual {
            vec![map]
        } else {
            vec![]
        }),
    }
}
fn match_unordered(
    expected: &[Value],
    actual: &[Value],
    index: usize,
    used: &mut [bool],
    map: Bijection,
    path: &str,
    depth: usize,
    search: &mut Search,
) -> Result<Vec<Bijection>, String> {
    search.step(depth)?;
    if index == expected.len() {
        return Ok(vec![map]);
    }
    let mut out = vec![];
    for i in 0..actual.len() {
        if used[i] {
            continue;
        }
        let candidates = match_value(
            &expected[index],
            &actual[i],
            map.clone(),
            &pointer(path, &index.to_string()),
            depth + 1,
            search,
        )?;
        used[i] = true;
        for candidate in candidates {
            out.extend(match_unordered(
                expected,
                actual,
                index + 1,
                used,
                candidate,
                path,
                depth + 1,
                search,
            )?);
            if out.len() > search.limits.mappings {
                out = search.unique(out)?;
            }
        }
        used[i] = false;
    }
    search.unique(out)
}

pub(super) struct GlobalComparison {
    candidates: Vec<Bijection>,
    limits: Limits,
    compared: usize,
    failed: usize,
}
impl Default for GlobalComparison {
    fn default() -> Self {
        Self::with_limits(Limits::default())
    }
}
impl GlobalComparison {
    pub(super) fn with_limits(limits: Limits) -> Self {
        Self {
            candidates: vec![Bijection::default()],
            limits,
            compared: 0,
            failed: 0,
        }
    }
    /// Failed observations remain failed. They do not erase the mappings proven
    /// by earlier matching rows; execution and recovery observations continue.
    pub(super) fn compare(&mut self, expected: &Value, actual: &Value) -> Value {
        self.compared += 1;
        let mut search = Search {
            limits: self.limits,
            steps: 0,
        };
        let result = (|| {
            check_observation_sort(expected).map_err(|e| format!("expected_{e}"))?;
            check_observation_sort(actual).map_err(|e| format!("actual_{e}"))?;
            let mut next = vec![];
            for map in &self.candidates {
                next.extend(match_value(
                    expected,
                    actual,
                    map.clone(),
                    "",
                    0,
                    &mut search,
                )?);
                if next.len() > search.limits.mappings {
                    next = search.unique(next)?;
                }
            }
            search.unique(next)
        })();
        let (matched, reason) = match result {
            Ok(maps) if !maps.is_empty() => {
                self.candidates = maps;
                (true, None)
            }
            Ok(_) => (
                false,
                Some("no_single_consistent_bijection_and_complete_record_match".to_owned()),
            ),
            Err(reason) => (false, Some(reason)),
        };
        if !matched {
            self.failed += 1;
        }
        json!({"matched":matched,"reason":reason,"search_steps":search.steps,"viable_global_mappings":self.candidates.len(),"compared":self.compared,"failed":self.failed})
    }
    pub(super) fn summary(&self) -> Value {
        json!({"compared":self.compared,"failed":self.failed,"global_bijection_candidates":self.candidates.iter().map(|v| &v.expected_to_actual).collect::<Vec<_>>(),"limits":{"search_steps_per_action":self.limits.search_steps,"mapping_candidates":self.limits.mappings,"depth":self.limits.depth}})
    }
}

#[cfg(test)]
mod controls {
    use super::*;
    fn blank(id: &str) -> Value {
        json!({"value":id,"term_type":"blank_node","datatype":null,"language":null})
    }
    fn iri(id: &str) -> Value {
        json!({"value":id,"term_type":"iri","datatype":null,"language":null})
    }
    fn row(quads: Vec<Value>, contexts: Vec<Value>) -> Value {
        let mut q = quads;
        let mut c = contexts;
        sort_observation(&mut q, false).unwrap();
        sort_observation(&mut c, true).unwrap();
        json!({"id":"test","operation":"new","status":"completed","raw":{},"state":{"quads":q,"contexts":c,"revision":1,"query_cache_entries":1,"snapshot_cached":true,"quad_count":1}})
    }
    fn one(id: &str) -> Value {
        row(
            vec![json!([blank(id), iri("urn:p"), iri("urn:o"), null])],
            vec![blank(id)],
        )
    }
    fn ok(c: &mut GlobalComparison, e: &Value, a: &Value) -> bool {
        c.compare(e, a)["matched"] == true
    }
    #[test]
    fn source_update_wrapper_comparator_legal_rename_reverses_observer_sort() {
        let e = row(
            vec![
                json!([blank("a"), iri("urn:p"), iri("urn:left"), null]),
                json!([blank("b"), iri("urn:p"), iri("urn:right"), null]),
            ],
            vec![blank("a"), blank("b")],
        );
        let a = row(
            vec![
                json!([blank("z"), iri("urn:p"), iri("urn:left"), null]),
                json!([blank("y"), iri("urn:p"), iri("urn:right"), null]),
            ],
            vec![blank("z"), blank("y")],
        );
        assert!(ok(&mut GlobalComparison::default(), &e, &a));
    }
    #[test]
    fn source_update_wrapper_comparator_merge_split_and_cross_new_remap_rejected() {
        let e = row(
            vec![json!([blank("a"), iri("urn:p"), blank("b"), null])],
            vec![],
        );
        let merged = row(
            vec![json!([blank("x"), iri("urn:p"), blank("x"), null])],
            vec![],
        );
        assert!(!ok(&mut GlobalComparison::default(), &e, &merged));
        assert!(!ok(&mut GlobalComparison::default(), &merged, &e));
        let mut c = GlobalComparison::default();
        assert!(ok(&mut c, &one("a"), &one("x")));
        assert!(!ok(&mut c, &one("a"), &one("y"))); // `new` never resets it.
        assert!(ok(&mut c, &one("a"), &one("x"))); // failed row did not erase it.
    }
    #[test]
    fn source_update_wrapper_comparator_retains_ambiguous_maps_until_later_row() {
        let e = row(
            vec![
                json!([blank("a"), iri("urn:p"), iri("urn:o"), null]),
                json!([blank("b"), iri("urn:p"), iri("urn:o"), null]),
            ],
            vec![],
        );
        let a = row(
            vec![
                json!([blank("x"), iri("urn:p"), iri("urn:o"), null]),
                json!([blank("y"), iri("urn:p"), iri("urn:o"), null]),
            ],
            vec![],
        );
        let mut c = GlobalComparison::default();
        assert!(ok(&mut c, &e, &a));
        assert_eq!(c.candidates.len(), 2);
        assert!(ok(&mut c, &one("a"), &one("y")));
        assert_eq!(c.candidates.len(), 1);
        assert!(!ok(&mut c, &one("b"), &one("y")));
    }
    #[test]
    fn source_update_wrapper_comparator_select_rows_values_and_snapshot_stay_ordered() {
        let mut e = one("a");
        e["raw"] = json!({"rows":[{"values":[["x",iri("urn:a")],["y",iri("urn:b")]]},{"values":[["x",iri("urn:b")],["y",iri("urn:a")]]}]});
        let mut a = e.clone();
        a["raw"]["rows"].as_array_mut().unwrap().swap(0, 1);
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        let mut a = e.clone();
        a["raw"]["rows"][0]["values"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        e["raw"] = json!({"canonical_nquads":"_:a <urn:p> <urn:o> .","dataset_sha256":"one"});
        let mut a = e.clone();
        a["raw"]["canonical_nquads"] = json!("_:b <urn:p> <urn:o> .");
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        a = e.clone();
        a["raw"]["dataset_sha256"] = json!("two");
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
    }
    #[test]
    fn source_update_wrapper_comparator_empty_context_error_and_cache_changes_rejected() {
        let mut e = one("a");
        e["state"]["contexts"]
            .as_array_mut()
            .unwrap()
            .push(iri("urn:empty"));
        sort_observation(e["state"]["contexts"].as_array_mut().unwrap(), true).unwrap();
        let mut a = e.clone();
        a["state"]["contexts"]
            .as_array_mut()
            .unwrap()
            .retain(|x| x["value"] != "urn:empty");
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        for (key, value) in [
            ("query_cache_entries", json!(0)),
            ("snapshot_cached", json!(false)),
            ("revision", json!(2)),
            ("quad_count", json!(2)),
        ] {
            let mut a = e.clone();
            a["state"][key] = value;
            assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        }
        e["error"] = json!({"module":"builtins","type":"ValueError","message":"original"});
        for key in ["module", "type", "message"] {
            let mut a = e.clone();
            a["error"][key] = json!("changed");
            assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        }
    }
    #[test]
    fn source_update_wrapper_comparator_duplicate_members_and_raw_literal_not_renamed() {
        let e = one("a");
        let mut a = e.clone();
        a["state"]["quads"]
            .as_array_mut()
            .unwrap()
            .push(e["state"]["quads"][0].clone());
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
        let mut e = one("a");
        let mut a = one("x");
        e["raw"] = json!({"term_type":"literal","value":"a"});
        a["raw"] = json!({"term_type":"literal","value":"x"});
        assert!(!ok(&mut GlobalComparison::default(), &e, &a));
    }
    #[test]
    fn source_update_wrapper_comparator_budget_exhaustion_never_passes() {
        let mut c = GlobalComparison::with_limits(Limits {
            search_steps: 0,
            mappings: 1,
            depth: 1,
        });
        let r = c.compare(&one("a"), &one("a"));
        assert_eq!(r["matched"], false);
        assert_eq!(r["reason"], "comparison_search_budget");
    }
    #[test]
    fn source_update_wrapper_comparator_observer_json_sort_unicode_and_spacing() {
        assert_eq!(
            observer_key(&json!({"z":"中😀","a":null}), false).unwrap(),
            "{\"a\": null, \"z\": \"中😀\"}"
        );
        assert_eq!(
            observer_key(&json!({"z":"中😀","a":null}), true).unwrap(),
            "{\"a\": null, \"z\": \"\\u4e2d\\ud83d\\ude00\"}"
        );
        let e = row(vec![], vec![iri("urn:a"), iri("urn:z")]);
        let mut a = e.clone();
        a["state"]["contexts"].as_array_mut().unwrap().reverse();
        let r = GlobalComparison::default().compare(&e, &a);
        assert_eq!(r["matched"], false);
        assert_eq!(
            r["reason"],
            "actual_original_observer_order:/state/contexts"
        );
    }
}
