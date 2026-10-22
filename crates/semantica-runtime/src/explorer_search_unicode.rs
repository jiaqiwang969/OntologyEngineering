//! Pinned Python 3.13 Unicode 15.1 lower data; no production Python delegate.
use std::{collections::BTreeMap, sync::OnceLock};
#[derive(serde::Deserialize)]
struct Data {
    lower: BTreeMap<u32, String>,
    sigma_cased: Vec<[u32; 2]>,
    sigma_ignorable: Vec<[u32; 2]>,
}
fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("data/explorer_search_unicode.json"))
            .expect("pinned Unicode data")
    })
}
fn contains(ranges: &[[u32; 2]], cp: u32) -> bool {
    let i = ranges.partition_point(|r| r[1] < cp);
    ranges.get(i).is_some_and(|r| r[0] <= cp)
}
fn lower_each<E, F: FnMut(u32) -> Result<(), E>>(points: &[u32], mut emit: F) -> Result<(), E> {
    let d = data();
    let mut next = vec![false; points.len()];
    let mut after = false;
    for (i, &cp) in points.iter().enumerate().rev() {
        next[i] = after;
        if !contains(&d.sigma_ignorable, cp) {
            after = contains(&d.sigma_cased, cp);
        }
    }
    let mut before = false;
    for (i, &cp) in points.iter().enumerate() {
        if cp == 0x3a3 && before && !next[i] {
            emit(0x3c2)?;
        } else if let Some(value) = d.lower.get(&cp) {
            for c in value.chars() {
                emit(c as u32)?;
            }
        } else {
            emit(cp)?;
        }
        if !contains(&d.sigma_ignorable, cp) {
            before = contains(&d.sigma_cased, cp);
        }
    }
    Ok(())
}
pub(super) fn lower(s: &str) -> String {
    let points = s.chars().map(u32::from).collect::<Vec<_>>();
    let mut out = String::with_capacity(s.len());
    let result: Result<(), std::convert::Infallible> = lower_each(&points, |cp| {
        out.push(char::from_u32(cp).expect("scalar lowercase"));
        Ok(())
    });
    match result {
        Ok(()) => out,
        Err(e) => match e {},
    }
}
pub(super) fn lower_points(points: &[u32], limit: usize) -> anyhow::Result<Vec<u32>> {
    anyhow::ensure!(
        points.iter().all(|&cp| cp <= 0x10ffff),
        "explorer_native_codepoint_invalid"
    );
    let mut out = Vec::new();
    lower_each(points, |cp| {
        anyhow::ensure!(out.len() < limit, "explorer_native_text_limit");
        out.push(cp);
        Ok(())
    })?;
    Ok(out)
}
