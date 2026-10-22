use oe_nx_worker::assembly_motion::binding::*;
fn occurrence(part: u32, body: u32) -> (ExpectedBody, QueriedBody) {
    (
        ExpectedBody {
            prototype_body: 21,
            prototype_owner: 2,
            part_occurrence: Some(part),
        },
        QueriedBody {
            body,
            is_occurrence: true,
            prototype_body: 21,
            containing_occurrence: part,
            owner: 1,
            is_solid_body: true,
        },
    )
}
#[test]
fn repeated_prototype_requires_distinct_native_placements() {
    let a = occurrence(100, 201);
    let b = occurrence(101, 202);
    validate_unique_bindings(&[a.clone(), b]).unwrap();
    let aliased = occurrence(101, 201);
    assert!(validate_unique_bindings(&[a.clone(), aliased]).is_err());
    assert!(validate_unique_bindings(&[a.clone(), a]).is_err());
}
#[test]
fn prototype_and_containing_occurrence_are_both_required() {
    let (e, good) = occurrence(100, 201);
    validate_binding(&e, &good).unwrap();
    for fault in 0..7 {
        let mut a = good.clone();
        match fault {
            0 => a.body = 0,
            1 => a.is_occurrence = false,
            2 => a.prototype_body = 22,
            3 => a.containing_occurrence = 101,
            4 => a.owner = 0,
            5 => a.is_solid_body = false,
            6 => a.body = 21,
            _ => unreachable!(),
        }
        assert!(validate_binding(&e, &a).is_err(), "{fault}");
    }
    let mut e = e;
    e.part_occurrence = Some(0);
    assert!(validate_binding(&e, &good).is_err());
}
#[test]
fn root_body_stays_direct_and_owned_by_root() {
    let e = ExpectedBody {
        prototype_body: 11,
        prototype_owner: 1,
        part_occurrence: None,
    };
    let good = QueriedBody {
        body: 11,
        is_occurrence: false,
        prototype_body: 11,
        containing_occurrence: 0,
        owner: 1,
        is_solid_body: true,
    };
    validate_binding(&e, &good).unwrap();
    for fault in 0..4 {
        let mut a = good.clone();
        match fault {
            0 => a.is_occurrence = true,
            1 => a.body = 12,
            2 => a.containing_occurrence = 100,
            3 => a.owner = 2,
            _ => unreachable!(),
        };
        assert!(validate_binding(&e, &a).is_err());
    }
}
