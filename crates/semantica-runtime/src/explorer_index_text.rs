//! Internal codepoint text. Legacy wire remains UTF-8 strings; native wire is
//! explicit. Ordering follows original Unicode codepoints, including surrogates.
use anyhow::{Context, Result};
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Text(pub(super) Vec<u32>);
impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Self(s.chars().map(u32::from).collect())
    }
}
impl From<String> for Text {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}
impl Text {
    pub(super) fn utf8(&self) -> Result<String> {
        self.0
            .iter()
            .map(|&cp| char::from_u32(cp).context("explorer_native_text_requires_native_wire"))
            .collect()
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub(super) fn contains(&self, needle: &Self) -> bool {
        let n = &needle.0;
        if n.is_empty() {
            return true;
        }
        if n.len() > self.0.len() {
            return false;
        }
        // Linear matching avoids a quadratic scan on long repetitive native text.
        let mut prefix = vec![0; n.len()];
        let mut k = 0;
        for i in 1..n.len() {
            while k > 0 && n[i] != n[k] {
                k = prefix[k - 1];
            }
            if n[i] == n[k] {
                k += 1;
            }
            prefix[i] = k;
        }
        k = 0;
        for &cp in &self.0 {
            while k > 0 && cp != n[k] {
                k = prefix[k - 1];
            }
            if cp == n[k] {
                k += 1;
                if k == n.len() {
                    return true;
                }
            }
        }
        false
    }
}
impl serde::Serialize for Text {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.utf8().map_err(serde::ser::Error::custom)?)
    }
}
impl<'de> serde::Deserialize<'de> for Text {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(String::deserialize(d)?.into())
    }
}
