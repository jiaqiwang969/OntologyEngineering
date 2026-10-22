//! Explicit owner protocol controls, not an original-library observation.
//! None and Some(empty) differ in retry events; empty thaw preserves outer
//! bindings. Some(marker) is the nonempty projected retry that changes output.
use super::*;
use crate::numeric::SourceHashPolicy;
use std::cell::RefCell;

type Row = InternalTuple<&'static str>;
type Event = SourceContextEvent<&'static str>;

fn view(row: &Row) -> Vec<(usize, &'static str)> {
    row.source_local_order()
        .iter()
        .map(|slot| {
            (
                *slot,
                *row.get(*slot).expect("test contains only RDF cells"),
            )
        })
        .collect()
}

fn exercise(scope: Option<Rc<[usize]>>) -> (Vec<Row>, Vec<Event>, Row) {
    let initial = Rc::new(Row::with_capacity(2));
    let variables: Rc<[Variable]> = vec![
        Variable::new("x").unwrap(),
        Variable::new("marker").unwrap(),
    ]
    .into();
    let hash = Rc::new(SourceHashRuntime::new(SourceHashPolicy::Seeded(0)).unwrap());
    let hash_cell = Rc::clone(&hash);
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&events);
    let runtime = Rc::new(
        SourceContextRuntime::new(
            variables,
            Rc::clone(&initial),
            CancellationToken::new(),
            hash,
            Rc::new(|a, b| Ok(a == b)),
            Rc::new(move |value: &&'static str| Ok(hash_cell.variable_hash(value))),
        )
        .with_observer(Rc::new(move |event| sink.borrow_mut().push(event))),
    );
    let mut outer = Row::with_capacity(2);
    outer.set(0, "urn:outside");
    let from = outer.source_context_from_bindings(initial);
    let mut frozen_left = from.source_solution(None);
    frozen_left.set(1, "urn:kept");
    let original_left = frozen_left.clone();
    let left: InternalTupleEvaluator<'static, &'static str> = Rc::new(move |current| {
        assert_eq!(view(&current), vec![(0, "urn:outside")]);
        Ok(Box::new(std::iter::once(Ok(frozen_left.clone()))))
    });
    // Actual production context assignment implements the AlreadyBound branch.
    // This controlled child supplies one VALUES assignment, not another OPTIONAL.
    let right: InternalTupleEvaluator<'static, &'static str> = Rc::new(|mut current| {
        let assigned = current.source_assign_by(0, SolutionValue::Rdf("urn:other"), |a, b| {
            Ok::<_, QueryEvaluationError>(a == b)
        })?;
        let row = assigned.then(|| current.source_solution(None));
        Ok(Box::new(row.into_iter().map(Ok)))
    });
    let evaluator =
        source_optional_evaluator(runtime, 17, left, right, scope, Rc::new(|_| Ok(true)));
    let mut stream = evaluator(from).unwrap();
    let rows: Vec<_> = stream.by_ref().collect::<Result<_, _>>().unwrap();
    assert!(stream.next().is_none());
    drop(stream);
    let captured = events.borrow().clone();
    (rows, captured, original_left)
}

fn starts(events: &[Event]) -> Vec<(SourceChildSide, u8, Vec<(usize, &'static str)>)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::ChildStart {
                node,
                side,
                probe,
                current,
                initial,
            } => {
                assert_eq!(*node, 17);
                assert!(view(initial).is_empty());
                Some((*side, *probe, view(current)))
            }
            _ => None,
        })
        .collect()
}

fn retries(
    events: &[Event],
) -> Vec<(
    Vec<(usize, &'static str)>,
    Vec<(usize, &'static str)>,
    Vec<(usize, &'static str)>,
)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::OptionalRetry {
                node,
                left_row,
                remembered_row,
                thawed_current,
                initial,
            } => {
                assert_eq!(*node, 17);
                assert!(view(initial).is_empty());
                Some((view(left_row), view(remembered_row), view(thawed_current)))
            }
            _ => None,
        })
        .collect()
}

fn right_polls(events: &[Event]) -> Vec<(u8, usize)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::ChildPoll {
                node,
                side: SourceChildSide::Right,
                probe,
                ordinal,
            } => {
                assert_eq!(*node, 17);
                Some((*probe, *ordinal))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn source_update_optional_none_returns_left_without_retry() {
    let (rows, events, original) = exercise(None);
    assert_eq!(rows.len(), 1);
    assert_eq!(view(&rows[0]), vec![(0, "urn:outside"), (1, "urn:kept")]);
    assert!(rows[0].source_same_object(&original));
    assert_eq!(
        starts(&events),
        vec![
            (SourceChildSide::Left, 0, vec![(0, "urn:outside")]),
            (
                SourceChildSide::Right,
                1,
                vec![(0, "urn:outside"), (1, "urn:kept")]
            ),
        ]
    );
    assert!(retries(&events).is_empty());
    assert_eq!(right_polls(&events), vec![(1, 1)]);
}

#[test]
fn source_update_optional_some_empty_retries_with_outer_fallback() {
    let (rows, events, original) = exercise(Some(Rc::from([])));
    assert_eq!(rows.len(), 1);
    assert_eq!(view(&rows[0]), vec![(0, "urn:outside"), (1, "urn:kept")]);
    assert!(rows[0].source_same_object(&original));
    assert_eq!(
        starts(&events),
        vec![
            (SourceChildSide::Left, 0, vec![(0, "urn:outside")]),
            (
                SourceChildSide::Right,
                1,
                vec![(0, "urn:outside"), (1, "urn:kept")]
            ),
            (SourceChildSide::Right, 2, vec![(0, "urn:outside")]),
        ]
    );
    assert_eq!(
        retries(&events),
        vec![(
            vec![(0, "urn:outside"), (1, "urn:kept")],
            vec![],
            vec![(0, "urn:outside")],
        )]
    );
    assert_eq!(right_polls(&events), vec![(1, 1), (2, 1)]);
}

#[test]
fn source_update_optional_some_marker_retry_suppresses_left_and_short_circuits() {
    let (rows, events, _) = exercise(Some(Rc::from([1])));
    assert!(rows.is_empty());
    assert_eq!(
        starts(&events),
        vec![
            (SourceChildSide::Left, 0, vec![(0, "urn:outside")]),
            (
                SourceChildSide::Right,
                1,
                vec![(0, "urn:outside"), (1, "urn:kept")]
            ),
            (SourceChildSide::Right, 2, vec![(1, "urn:kept")]),
        ]
    );
    assert_eq!(
        retries(&events),
        vec![(
            vec![(0, "urn:outside"), (1, "urn:kept")],
            vec![(1, "urn:kept")],
            vec![(1, "urn:kept")],
        )]
    );
    assert_eq!(right_polls(&events), vec![(1, 1), (2, 1)]);
    let second_rows: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Event::ChildRow {
                side: SourceChildSide::Right,
                probe: 2,
                ordinal,
                row,
                ..
            } => {
                assert_eq!(*ordinal, 1);
                Some(view(row))
            }
            _ => None,
        })
        .collect();
    assert_eq!(second_rows, vec![vec![(1, "urn:kept"), (0, "urn:other")]]);
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::ChildEnd {
            side: SourceChildSide::Right,
            probe: 2,
            ..
        }
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::ChildDropped {
            side: SourceChildSide::Right,
            probe: 2,
            polls: 1,
            ..
        }
    )));
}
