// Deliberately uses repeated lookup so a static tool has a concrete candidate.
// A finding here is not a measured performance improvement.
export function joinLabels(ids, rows) {
  return ids.map((id) => {
    const row = rows.find((candidate) => candidate.id === id);
    return row ? row.label : "missing";
  });
}

export function fixtureResult() {
  return joinLabels([2, 1, 9], [
    { id: 1, label: "alpha" },
    { id: 2, label: "beta" },
  ]);
}

// Explicit loops exercise the pinned structural scanner's supported syntax.
export function countMatches(left, right) {
  let count = 0;
  for (const leftValue of left) {
    for (const rightValue of right) {
      if (leftValue === rightValue) count += 1;
    }
  }
  return count;
}
