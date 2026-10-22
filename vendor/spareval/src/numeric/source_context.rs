//! Immutable context/hash policy shared by a prepared source query.
use super::*;
use crate::source_hash::{self, SourceHashSecret};
use rand::TryRngCore;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QueryContextSemantics {
    LegacyPrivateV1,
    SourceContextV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceContextFault {
    EmptyValuesAttribute,
    NullGroupKey,
    HashEntropy,
}
impl fmt::Display for SourceContextFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyValuesAttribute => "AttributeError: 'list' object has no attribute 'name'",
            Self::NullGroupKey => "Exception: Cannot eval thing: None (<class 'NoneType'>)",
            Self::HashEntropy => "source hash runtime entropy is unavailable",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceHashPolicy {
    NativeRandom,
    Seeded(u32),
}

pub(crate) struct SourceHashRuntime {
    policy: SourceHashPolicy,
    secret: SourceHashSecret,
}
impl fmt::Debug for SourceHashRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceHashRuntime")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}
impl SourceHashRuntime {
    pub(crate) fn new(policy: SourceHashPolicy) -> Result<Self, RawNumericFault> {
        let secret = match policy {
            SourceHashPolicy::Seeded(seed) => source_hash::secret_from_seed(seed),
            SourceHashPolicy::NativeRandom => {
                // A process owns one secret. Concurrent first constructors use
                // the winning value; an entropy failure is never a zero seed.
                static SECRET: OnceLock<[u8; 24]> = OnceLock::new();
                if SECRET.get().is_none() {
                    let mut bytes = [0; 24];
                    rand::rngs::OsRng.try_fill_bytes(&mut bytes).map_err(|_| {
                        RawNumericFault::SourceContext(SourceContextFault::HashEntropy)
                    })?;
                    let _ = SECRET.set(bytes);
                }
                source_hash::SourceHashSecret::from_bytes(*SECRET.get().ok_or(
                    RawNumericFault::SourceContext(SourceContextFault::HashEntropy),
                )?)
            }
        };
        Ok(Self { policy, secret })
    }
    pub(crate) fn policy(&self) -> SourceHashPolicy { self.policy }
    pub(crate) fn variable_hash(&self, name: &str) -> i64 {
        source_hash::hash_python_string(name, &self.secret)
    }
    pub(crate) fn term_hash(&self, raw: &RawExpressionValue) -> Result<i64, RawNumericFault> {
        if let Some(v) = raw.source_variable() { return Ok(self.variable_hash(v.as_str())); }
        Ok(match raw.rdf()? {
            Term::NamedNode(node) => source_hash::hash_python_string(node.as_str(), &self.secret),
            Term::BlankNode(node) => source_hash::hash_python_string(node.as_str(), &self.secret),
            Term::Literal(literal) => {
                let LiteralSourceState::Literal(metadata) = raw.literal_source() else {
                    return Err(RawNumericFault::InvalidSourceOrigin);
                };
                // Same source order: str.__hash__, then language, then datatype.
                let mut word = source_hash::hash_python_string(literal.value(), &self.secret);
                match &metadata.language_original {
                    SourceFact::Known(Some(language)) if !language.is_empty() => {
                        word ^= source_hash::hash_python_string(
                            &language.to_ascii_lowercase(),
                            &self.secret,
                        );
                    }
                    SourceFact::Known(_) => (),
                    SourceFact::Unknown => {
                        return Err(RawNumericFault::UnknownSourceLiteralIdentity);
                    }
                }
                match &metadata.datatype {
                    SourceDatatype::Absent => (),
                    SourceDatatype::Specified(node) => {
                        word ^= source_hash::hash_python_string(node.as_str(), &self.secret);
                    }
                    SourceDatatype::Unknown => {
                        return Err(RawNumericFault::UnknownSourceLiteralIdentity);
                    }
                }
                source_hash::normalize_hash_word(word)
            }
            #[cfg(feature = "sparql-12")]
            Term::Triple(_) => {
                return Err(RawNumericFault::UnsupportedSourceLiteralOperator(
                    "triple source hash",
                ));
            }
        })
    }
}
