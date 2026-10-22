/// A generated strong model view is data, not proof of schema validation or
/// engineering adoption. Runtime verifies source identity before conversion.
pub(crate) mod sealed {
    pub trait Sealed {}
}

pub trait SemanticModelSnapshot:
    sealed::Sealed + serde::Serialize + serde::de::DeserializeOwned
{
    const MODEL_ID: &'static str;
    const SOURCE_SHA256: &'static str;
    const CLASS_SHA256: &'static str;
}
