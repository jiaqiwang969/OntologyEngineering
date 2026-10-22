use std::collections::BTreeSet;
fn legacy_impact(design: &oe_design::Design, changed: &BTreeSet<String>) -> BTreeSet<String> {
    let mut affected = changed.clone();
    loop {
        let before = affected.len();
        for model in &design.semantic_models {
            if affected.contains(&format!("semantic_model_{}", model.rust_name)) {
                affected.insert("semantic_schema".into());
                affected.insert("semantic_typed".into());
                affected.insert("semantic_contracts".into());
            }
        }
        for t in &design.types {
            if t.fields.iter().any(|f| {
                design.types.iter().any(|other| {
                    affected.contains(&format!("type_{}", other.name))
                        && type_mentions(&f.rust_type, &other.name)
                })
            }) {
                affected.insert(format!("type_{}", t.name));
            }
        }
        for i in &design.interfaces {
            if affected.contains(&format!("interface_{}", i.name))
                || design.types.iter().any(|t| {
                    affected.contains(&format!("type_{}", t.name))
                        && [&i.input, &i.output, &i.error]
                            .into_iter()
                            .any(|s| type_mentions(s, &t.name))
                })
            {
                affected.insert(format!("interface_{}", i.name));
                affected.insert(i.module.clone());
            }
        }
        for m in &design.machines {
            if affected.contains(&format!("machine_{}", m.name)) {
                affected.insert(m.module.clone());
            }
        }
        for m in &design.modules {
            if affected.contains("model")
                || m.depends_on
                    .iter()
                    .chain(m.realizes.iter())
                    .chain(m.source_refs.iter())
                    .any(|x| affected.contains(x))
            {
                affected.insert(m.id.clone());
            }
        }
        if affected.len() == before {
            break;
        }
    }
    for c in &design.constraints {
        if affected.contains(&c.module) || affected.contains(&c.requirement) {
            affected.insert(c.id.clone());
        }
    }
    for v in &design.validations {
        if affected.contains(&v.constraint) || affected.contains(&v.module) {
            affected.insert(v.id.clone());
        }
    }
    affected
}
fn type_mentions(ty: &str, name: &str) -> bool {
    ty.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|t| t == name)
}
fn baseline() -> oe_design::Design {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    oe_design::load(&root.join("specs/engineering.toml")).unwrap()
}
fn set(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn fixture() -> oe_design::Design {
    let mut d = baseline();
    let t = d.types[0].clone();
    let field = t.fields[0].clone();
    let module = d.modules[0].clone();
    let mut port = d.interfaces[0].clone();
    let mut machine = d.machines[0].clone();
    let mut model = d.semantic_models[0].clone();
    let mut c = d.constraints[0].clone();
    let mut v = d.validations[0].clone();
    d.types.clear();
    d.modules.clear();
    d.interfaces.clear();
    d.machines.clear();
    d.semantic_models.clear();
    d.constraints.clear();
    d.validations.clear();
    for name in ["A", "B", "C", "AA"] {
        let mut row = t.clone();
        row.name = name.into();
        let mut f = field.clone();
        f.rust_type = "External".into();
        row.fields = vec![f];
        d.types.push(row);
    }
    for (id, deps, requirements, sources) in [
        ("m0", vec![], vec!["req"], vec!["source"]),
        ("m1", vec!["m0"], vec![], vec![]),
        ("late", vec!["late_constraint"], vec![], vec![]),
    ] {
        let mut m = module.clone();
        m.id = id.into();
        m.depends_on = deps.into_iter().map(str::to_owned).collect();
        m.realizes = requirements.into_iter().map(str::to_owned).collect();
        m.source_refs = sources.into_iter().map(str::to_owned).collect();
        d.modules.push(m);
    }
    port.name = "Port".into();
    port.module = "m0".into();
    port.input = "Vec<Option<A>>".into();
    port.output = "Result<C,Foreign>".into();
    port.error = "B".into();
    d.interfaces.push(port);
    machine.name = "Flow".into();
    machine.module = "m0".into();
    d.machines.push(machine);
    model.rust_name = "Model".into();
    d.semantic_models.push(model);
    c.id = "late_constraint".into();
    c.module = "m0".into();
    c.requirement = "other_req".into();
    d.constraints.push(c);
    v.id = "validation".into();
    v.constraint = "late_constraint".into();
    v.module = "other".into();
    d.validations.push(v);
    d
}
#[test]
fn exhaustive_type_graphs_match_legacy_closure() {
    let base = fixture();
    let mut comparisons = 0;
    for mask in 0u16..512 {
        let mut d = base.clone();
        for to in 0..3 {
            let refs = (0..3)
                .filter(|from| mask & (1 << (to * 3 + from)) != 0)
                .map(|i| ["A", "B", "C"][i])
                .collect::<Vec<_>>();
            d.types[to].fields[0].rust_type = format!("Map<{}, External>", refs.join(","));
        }
        for seeds in [
            set(&[]),
            set(&["type_A"]),
            set(&["type_B"]),
            set(&["type_C"]),
            set(&["type_AA"]),
            set(&["type_A", "source"]),
            set(&["machine_Flow"]),
            set(&["semantic_model_Model"]),
            set(&["unknown"]),
        ] {
            assert_eq!(
                d.impact(&seeds),
                legacy_impact(&d, &seeds),
                "mask={mask},seeds={seeds:?}"
            );
            comparisons += 1;
        }
        d.types.reverse();
        d.modules.reverse();
        d.interfaces.reverse();
        let seeds = set(&["type_A", "type_C"]);
        assert_eq!(d.impact(&seeds), legacy_impact(&d, &seeds));
        comparisons += 1;
    }
    assert_eq!(comparisons, 5120);
}
#[test]
fn propagation_domains_and_post_closure_order_remain_exact() {
    let d = fixture();
    for seed in ["req", "source", "machine_Flow", "interface_Port", "type_A"] {
        assert_eq!(
            d.impact(&set(&[seed])),
            set(&[
                seed,
                "interface_Port",
                "m0",
                "m1",
                "late_constraint",
                "validation"
            ])
            .difference(&if matches!(seed, "req" | "source" | "machine_Flow") {
                set(&["interface_Port"])
            } else {
                set(&[])
            })
            .cloned()
            .collect()
        );
    }
    assert_eq!(d.impact(&set(&["type_AA"])), set(&["type_AA"]));
    assert_eq!(
        d.impact(&set(&["semantic_model_Model"])),
        set(&[
            "semantic_model_Model",
            "semantic_schema",
            "semantic_typed",
            "semantic_contracts"
        ])
    );
    assert_eq!(
        d.impact(&set(&["model"])),
        set(&["model", "m0", "m1", "late", "late_constraint", "validation"])
    );
    // A constraint introduced in the ordered final pass must not propagate back
    // into module closure, but an explicitly changed constraint is an input seed.
    assert!(!d.impact(&set(&["m0"])).contains("late"));
    assert_eq!(
        d.impact(&set(&["late_constraint"])),
        set(&["late_constraint", "late", "validation"])
    );
    let mut ordered = d.clone();
    let mut c = d.constraints[0].clone();
    c.id = "downstream".into();
    c.module = "late_constraint".into();
    c.requirement = "none".into();
    ordered.constraints.push(c);
    assert!(ordered.impact(&set(&["m0"])).contains("downstream"));
    ordered.constraints.reverse();
    assert!(!ordered.impact(&set(&["m0"])).contains("downstream"));
    assert_eq!(
        ordered.impact(&set(&["m0"])),
        legacy_impact(&ordered, &set(&["m0"]))
    );
    let mut changed = d.clone();
    changed.types[0].fields[0].rust_type = "AA".into();
    assert!(changed.impact(&set(&["type_AA"])).contains("type_A"));
    assert!(!d.impact(&set(&["type_AA"])).contains("type_A")); // no stale cache across values
}
#[test]
fn current_full_design_matches_legacy_selected_roots() {
    let d = baseline();
    for seeds in [
        set(&["type_SemanticExplorerPathQuery"]),
        set(&["model"]),
        BTreeSet::from([format!("semantic_model_{}", d.semantic_models[0].rust_name)]),
        set(&["unknown"]),
        set(&[]),
    ] {
        let before = std::time::Instant::now();
        let expected = legacy_impact(&d, &seeds);
        let legacy = before.elapsed();
        let before = std::time::Instant::now();
        let actual = d.impact(&seeds);
        let candidate = before.elapsed();
        assert_eq!(actual, expected, "{seeds:?}");
        eprintln!(
            "impact roots={seeds:?} legacy={legacy:?} candidate={candidate:?} affected={}",
            actual.len()
        );
    }
}
