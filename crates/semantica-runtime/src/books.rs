//! Source-bounded book packages and normative engraving. Providers own transport
//! and application authorization; this module has no workspace or project state.
use anyhow::{Context, Result, bail, ensure};
pub use oxigraph::model::Term as RdfTerm;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub trait SourceProvider {
    fn read(&self, logical_id: &str) -> Result<Vec<u8>>;
    fn resolve_id(&self, logical_id: &str) -> Result<String> {
        Ok(logical_id.into())
    }
    fn read_scoped_asset(&self, root: &str, path: &str) -> Result<(Vec<u8>, String)> {
        let id = crate::packages::source_id(root, path)?;
        Ok((self.read(&id)?, id))
    }
    fn metadata_at(&self, bytes: &[u8], _logical_id: &str) -> Result<Value> {
        self.metadata(bytes)
    }
    /// Decode metadata from the exact bytes just read, never from a parallel file.
    fn metadata(&self, bytes: &[u8]) -> Result<Value> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

pub fn relative(value: &str) -> Result<&str> {
    ensure!(
        !value.is_empty()
            && !value.starts_with('/')
            && !value.contains(['\\', ':'])
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".."),
        "book_source_id_not_relative"
    );
    Ok(value)
}
fn join(parent: &str, child: &str) -> Result<String> {
    relative(child)?;
    if parent.is_empty() {
        Ok(child.into())
    } else {
        relative(parent)?;
        Ok(format!("{parent}/{child}"))
    }
}
fn parent(value: &str) -> &str {
    value.rsplit_once('/').map_or("", |p| p.0)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let s = value[key]
        .as_str()
        .with_context(|| format!("book_text_missing:{key}"))?;
    ensure!(!s.is_empty(), "book_text_empty:{key}");
    Ok(s)
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn metadata(provider: &dyn SourceProvider, id: &str) -> Result<(Vec<u8>, Value)> {
    let bytes = provider.read(id)?;
    ensure!(bytes.len() <= crate::MAX_BYTES, "book_metadata_budget");
    let value = provider.metadata(&bytes)?;
    ensure!(value.is_object(), "book_metadata_not_mapping");
    Ok((bytes, value))
}

#[derive(Clone)]
struct Package {
    descriptor: Value,
    manifest_id: String,
    manifest: Value,
}

/// A transient content view of a provider's registry, not a second registry or
/// mutable lifecycle store. Package counts and volumes come from the source.
pub struct Catalog {
    registry: Value,
    policy: Value,
    root: String,
    packages: BTreeMap<String, Package>,
    chapters: BTreeMap<(String, String), String>,
    domains: Vec<String>,
}
impl Catalog {
    /// A read-only content view of one caller-bound execution manifest. This is
    /// neither an industry registry nor a promotion/rights decision. Loading,
    /// scenario execution and receipts use the same package/Dataset algorithms.
    pub fn from_execution_manifest(manifest: &Value) -> Result<Self> {
        ensure!(manifest["schema_version"] == "1.0", "book_manifest_schema");
        let id = text(manifest, "package_id")?.to_owned();
        text(manifest, "version")?;
        ensure!(
            manifest["assets"].as_array().is_some_and(|v| !v.is_empty()),
            "book_manifest_assets_missing"
        );
        let descriptor = json!({"package_id":id,"version":manifest["version"],"content_profile":"caller-bound execution manifest; no registry or rights authority"});
        Ok(Self {
            registry: json!({"schema_version":"1.0","packages":[],"domain_packages":[descriptor.clone()]}),
            policy: json!({"schema_version":"1.0","profile":"caller-bound manifest observation; promotion and source rights unknown"}),
            root: String::new(),
            packages: BTreeMap::from([(
                id.clone(),
                Package {
                    descriptor,
                    manifest_id: "manifest.json".into(),
                    manifest: manifest.clone(),
                },
            )]),
            chapters: BTreeMap::new(),
            domains: vec![id],
        })
    }
    pub fn read(provider: &dyn SourceProvider, registry_id: &str) -> Result<Self> {
        relative(registry_id)?;
        let (_, registry) = metadata(provider, registry_id)?;
        ensure!(registry["schema_version"] == "1.0", "book_registry_schema");
        let root = parent(registry_id).to_owned();
        let (_, policy) = metadata(provider, &join(&root, text(&registry, "policy")?)?)?;
        let mut result = Self {
            registry: registry.clone(),
            policy,
            root,
            packages: BTreeMap::new(),
            chapters: BTreeMap::new(),
            domains: Vec::new(),
        };
        for (key, chapter) in [("packages", true), ("domain_packages", false)] {
            for d in registry[key]
                .as_array()
                .with_context(|| format!("book_registry_missing:{key}"))?
            {
                let id = text(d, "package_id")?.to_owned();
                let manifest_id = join(&result.root, text(d, "manifest")?)?;
                let (_, manifest) = metadata(provider, &manifest_id)?;
                ensure!(
                    manifest["package_id"] == id && manifest["version"] == d["version"],
                    "book_registry_manifest_identity"
                );
                if chapter {
                    let k = (
                        text(d, "volume")?.to_owned(),
                        text(d, "chapter")?.to_owned(),
                    );
                    ensure!(
                        result.chapters.insert(k, id.clone()).is_none(),
                        "book_duplicate_chapter"
                    );
                } else {
                    result.domains.push(id.clone());
                }
                ensure!(
                    result
                        .packages
                        .insert(
                            id,
                            Package {
                                descriptor: d.clone(),
                                manifest_id,
                                manifest
                            }
                        )
                        .is_none(),
                    "book_duplicate_package"
                );
            }
        }
        let expected = registry["expected_domain_package_count"]
            .as_u64()
            .unwrap_or(result.domains.len() as u64);
        ensure!(expected == result.domains.len() as u64, "book_domain_count");
        result.domains.sort();
        Ok(result)
    }
    pub fn list_chapter_packages(&self, volume: Option<&str>) -> Vec<Value> {
        self.chapters
            .iter()
            .filter(|((v, _), _)| volume.is_none_or(|wanted| wanted == v))
            .map(|(_, id)| self.packages[id].descriptor.clone())
            .collect()
    }
    pub fn list_domain_packages(&self) -> Vec<Value> {
        self.domains
            .iter()
            .map(|id| {
                let p = &self.packages[id];
                let mut d = p.descriptor.clone();
                d["domain"] = p.manifest["domain"].clone();
                d
            })
            .collect()
    }
    pub fn chapter_package_id(&self, volume: &str, chapter: &str) -> Result<&str> {
        self.chapters
            .get(&(volume.into(), chapter.into()))
            .map(String::as_str)
            .context("book_chapter_not_registered")
    }
    pub fn read_manifest(&self, package_id: &str) -> Result<&Value> {
        Ok(&self
            .packages
            .get(package_id)
            .context("book_package_not_registered")?
            .manifest)
    }
    pub fn read_policy(&self) -> &Value {
        &self.policy
    }
    pub fn asset(
        &self,
        provider: &dyn SourceProvider,
        package_id: &str,
        asset_id: &str,
    ) -> Result<Vec<u8>> {
        let p = self
            .packages
            .get(package_id)
            .context("book_package_not_registered")?;
        let matches: Vec<_> = p.manifest["assets"]
            .as_array()
            .context("book_assets_missing")?
            .iter()
            .filter(|a| a["asset_id"].as_str().map(str::trim) == Some(asset_id))
            .collect();
        ensure!(matches.len() == 1, "book_asset_not_uniquely_declared");
        let a = matches[0];
        Ok(crate::packages::read_asset(
            provider,
            Some(parent(&p.manifest_id)),
            a,
            &BTreeMap::new(),
            crate::packages::AssetPolicy::RequireExpectedSha,
        )?
        .content)
    }
    pub fn package_asset_text(
        &self,
        provider: &dyn SourceProvider,
        package_id: &str,
        asset_id: &str,
    ) -> Result<Value> {
        // Public lookup verifies the entire package, including unselected
        // assets and all loaded RDF, before returning the selected bytes.
        self.load_package(
            provider,
            package_id,
            &mut crate::dataset::DatasetSession::new(),
        )?;
        let bytes = self.asset(provider, package_id, asset_id)?;
        let value = std::str::from_utf8(&bytes).context("book_asset_not_utf8")?;
        ensure!(!value.contains('\0'), "book_asset_binary");
        Ok(
            json!({"package_id":package_id,"asset_id":asset_id,"text":value,"sha256":crate::sha256(&bytes),"size_bytes":bytes.len()}),
        )
    }
    pub fn chapter_asset_text(
        &self,
        provider: &dyn SourceProvider,
        volume: &str,
        chapter: &str,
        asset_id: &str,
    ) -> Result<Value> {
        let mut value = self.package_asset_text(
            provider,
            self.chapter_package_id(volume, chapter)?,
            asset_id,
        )?;
        value["volume"] = json!(volume);
        value["chapter"] = json!(chapter);
        Ok(value)
    }
    fn contract(&self, provider: &dyn SourceProvider, package_id: &str) -> Result<Value> {
        let manifest = self.read_manifest(package_id)?;
        let a = manifest["assets"]
            .as_array()
            .context("book_assets_missing")?
            .iter()
            .find(|a| a["role"] == "chapter_contract")
            .context("book_chapter_contract_missing")?;
        let data = self.asset(provider, package_id, text(a, "asset_id")?)?;
        let contract = provider.metadata(&data)?;
        ensure!(contract.is_object(), "book_contract_not_mapping");
        Ok(contract)
    }
    pub fn load_package(
        &self,
        provider: &dyn SourceProvider,
        package_id: &str,
        session: &mut crate::dataset::DatasetSession,
    ) -> Result<Value> {
        let package = self
            .packages
            .get(package_id)
            .context("book_package_not_registered")?;
        let prepared = crate::packages::prepare(
            provider,
            &package.manifest,
            Some(parent(&package.manifest_id)),
            &BTreeMap::new(),
            crate::packages::AssetPolicy::RequireExpectedSha,
        )?;
        Ok(serde_json::to_value(
            session.register_prepared_package(prepared)?,
        )?)
    }
    pub fn load_chapter_package(
        &self,
        provider: &dyn SourceProvider,
        volume: &str,
        chapter: &str,
        session: &mut crate::dataset::DatasetSession,
    ) -> Result<Value> {
        self.load_package(provider, self.chapter_package_id(volume, chapter)?, session)
    }
    pub fn load_domain_package(
        &self,
        provider: &dyn SourceProvider,
        package_id: &str,
        session: &mut crate::dataset::DatasetSession,
    ) -> Result<Value> {
        ensure!(
            self.domains.iter().any(|id| id == package_id),
            "book_domain_not_registered"
        );
        self.load_package(provider, package_id, session)
    }
    pub fn execute_chapter_query(
        &self,
        provider: &dyn SourceProvider,
        volume: &str,
        chapter: &str,
        asset_id: &str,
        session: &mut crate::dataset::DatasetSession,
        namespaces: &BTreeMap<String, String>,
        bindings: &BTreeMap<String, oxigraph::model::Term>,
    ) -> Result<Value> {
        let id = self.chapter_package_id(volume, chapter)?;
        self.load_package(provider, id, session)?;
        let manifest = self.read_manifest(id)?;
        let a = manifest["assets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["asset_id"] == asset_id)
            .context("book_query_asset_unknown")?;
        ensure!(a["role"] == "sparql", "book_query_asset_not_sparql");
        let bytes = self.asset(provider, id, asset_id)?;
        let query = std::str::from_utf8(&bytes).context("book_query_not_utf8")?;
        Ok(session
            .query_chapter_with_options(query, namespaces, bindings)?
            .result)
    }
    pub fn read_migration_map(
        &self,
        provider: &dyn SourceProvider,
        volume: &str,
        source_id: &str,
        encoding: MigrationEncoding,
    ) -> Result<Value> {
        let (raw, map) = metadata(provider, source_id)?;
        ensure!(map["schema_version"] == "1.0", "book_migration_schema");
        let records = map[match encoding {
            MigrationEncoding::LegacyAssets => "assets",
            MigrationEncoding::SuccessorMappings => "mappings",
        }]
        .as_array()
        .context("book_migration_records_missing")?;
        if !map["mapping_count"].is_null() {
            ensure!(
                map["mapping_count"].as_u64() == Some(records.len() as u64),
                "book_migration_count"
            );
        }
        let mut entries = Vec::new();
        for r in records {
            ensure!(r.is_object(), "book_migration_record_not_mapping");
            let e = match encoding {
                MigrationEncoding::LegacyAssets => {
                    json!({"volume":volume,"old_path":r["legacy_path"],"old_sha256":r["sha256"],"package_id":r["package_id"],"asset_id":r["asset_id"],"successor_sha256":r["sha256"],"relation":r["migration"],"successor_path":r["semantica_path"].as_str().filter(|s|!s.is_empty()),"source_fragment":null,"strict_gate_asset":r["strict_gate_asset"].as_bool()})
                }
                MigrationEncoding::SuccessorMappings => {
                    json!({"volume":volume,"old_path":r["old_path"],"old_sha256":r["old_sha256"],"package_id":r["new_package"],"asset_id":r["new_asset_id"],"successor_sha256":r["new_sha256"],"relation":r["relation"],"successor_path":null,"source_fragment":r["source_fragment"],"strict_gate_asset":null})
                }
            };
            for k in ["old_path", "package_id", "asset_id", "relation"] {
                text(&e, k)?;
            }
            for k in ["old_sha256", "successor_sha256"] {
                ensure!(sha(text(&e, k)?), "book_migration_digest_invalid");
            }
            let package = text(&e, "package_id")?;
            let asset = text(&e, "asset_id")?;
            ensure!(
                crate::sha256(self.asset(provider, package, asset)?) == e["successor_sha256"],
                "book_migration_successor_digest"
            );
            if let Some(mapped) = e["successor_path"].as_str() {
                let p = &self.packages[package];
                let a = p.manifest["assets"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|a| a["asset_id"] == asset)
                    .unwrap();
                ensure!(
                    relative(mapped)? == join(parent(&p.manifest_id), text(a, "path")?)?,
                    "book_migration_successor_path"
                );
            }
            entries.push(e);
        }
        let mut retired = Vec::new();
        let mut paths = BTreeSet::new();
        if let Some(raw_retired) = map.get("retired_runtime_entries") {
            for e in raw_retired
                .as_array()
                .context("book_retired_entries_not_list")?
            {
                let path = text(e, "legacy_path")?;
                ensure!(
                    paths.insert(path.to_owned()) && e["copied_as_runtime"] == false,
                    "book_retired_entry_unsafe"
                );
                text(e, "replacement_entry")?;
                text(e, "reason")?;
                let replacements = e.get("replacement_assets").cloned().unwrap_or(json!([]));
                ensure!(
                    replacements
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_string)),
                    "book_retired_assets_not_list"
                );
                retired.push(json!({"legacy_path":path,"copied_as_runtime":false,"replacement_entry":e["replacement_entry"],"replacement_assets":replacements,"reason":e["reason"]}));
            }
        }
        Ok(
            json!({"schema_version":"1.0","volume":volume,"source_sha256":crate::sha256(raw),"entry_count":entries.len(),"entries":entries,"retired_runtime_entry_count":retired.len(),"retired_runtime_entries":retired}),
        )
    }
    pub fn resolve_successor(old_path: &str, maps: &[Value]) -> Result<Value> {
        let mut candidates = Vec::new();
        for map in maps {
            for e in map["entries"]
                .as_array()
                .context("book_migration_entries_missing")?
            {
                if e["old_path"] == old_path {
                    candidates.push(e.clone());
                }
            }
        }
        Ok(
            json!({"old_path":old_path,"resolved":!candidates.is_empty(),"ambiguous":candidates.len()>1,"candidates":candidates}),
        )
    }
    pub fn verify_book_source_bindings(
        &self,
        packages: &dyn SourceProvider,
        sources: &dyn SourceProvider,
        volume: Option<&str>,
        require_tex: bool,
        require_guide: bool,
    ) -> Result<Value> {
        let mut checks = Vec::new();
        for ((v, _), id) in &self.chapters {
            if volume.is_some_and(|wanted| wanted != v) {
                continue;
            }
            let m = &self.packages[id].manifest;
            let binding = if v == "vol1" {
                &m["chapter"]["external_specification"]
            } else {
                &m["book_source"]
            };
            let anchor = binding_text(binding, &["source_anchor", "logical_anchor"]);
            let expected = binding_text(binding, &["source_sha256", "sha256"]);
            checks.push(digest_check(
                sources,
                "book.primary.digest",
                id,
                anchor,
                expected,
            ));
            let external = match self.contract(packages, id) {
                Ok(c) => {
                    let external = c["external_specification"].clone();
                    checks.push(binding_check(
                        "book.contract.binding",
                        id,
                        anchor,
                        expected,
                        binding_text(&external, &["source_anchor", "logical_anchor"]),
                        binding_text(&external, &["source_sha256", "sha256"]),
                    ));
                    external
                }
                Err(e) => {
                    checks.push(json!({"check_id":"book.contract.binding","package_id":id,"source_anchor":"","expected_sha256":expected,"actual_sha256":null,"status":"blocked","reason":e.to_string()}));
                    Value::Null
                }
            };
            for (field, required) in [("tex", require_tex), ("guide", require_guide)] {
                let a_key = format!("{field}_anchor");
                let h_key = format!("{field}_sha256");
                let a = binding_text(binding, &[&a_key]);
                let h = binding_text(binding, &[&h_key]);
                if required || !a.is_empty() || !h.is_empty() {
                    checks.push(digest_check(
                        sources,
                        &format!("book.{field}.digest"),
                        id,
                        a,
                        h,
                    ));
                    checks.push(binding_check(
                        &format!("book.contract.{field}-binding"),
                        id,
                        a,
                        h,
                        binding_text(&external, &[&a_key]),
                        binding_text(&external, &[&h_key]),
                    ));
                }
            }
            if let Some(assets) = m["assets"].as_array() {
                for asset in assets {
                    if asset["source_anchor"] == anchor {
                        let asset_id = asset["asset_id"].as_str().unwrap_or("unknown");
                        checks.push(binding_check(
                            &format!("book.asset-source.{asset_id}"),
                            id,
                            anchor,
                            expected,
                            binding_text(asset, &["source_anchor"]),
                            binding_text(asset, &["source_sha256"]),
                        ));
                    }
                }
            }
        }
        let passed = !checks.is_empty() && checks.iter().all(|c| c["status"] == "passed");
        Ok(
            json!({"schema_version":"1.0","check_count":checks.len(),"checks":checks,"passed":passed,"status":if passed{"passed"}else{"blocked"}}),
        )
    }
    pub fn validate_domain_package(
        &self,
        provider: &dyn SourceProvider,
        id: &str,
    ) -> Result<Vec<String>> {
        ensure!(
            self.domains.iter().any(|d| d == id),
            "book_domain_not_registered"
        );
        self.asset_issues(provider, id, false)
    }
    fn asset_issues(
        &self,
        provider: &dyn SourceProvider,
        id: &str,
        source_binding: bool,
    ) -> Result<Vec<String>> {
        let m = self.read_manifest(id)?;
        let mut issues = Vec::new();
        let Some(assets) = m["assets"].as_array().filter(|a| !a.is_empty()) else {
            return Ok(vec!["manifest assets are missing".into()]);
        };
        let mut seen = BTreeSet::new();
        for a in assets {
            if !a.is_object() {
                issues.push("manifest asset is not a mapping".into());
                continue;
            }
            let asset_id = a["asset_id"].as_str().unwrap_or("");
            if asset_id.is_empty() {
                issues.push("manifest asset_id is missing".into());
                continue;
            }
            if !seen.insert(asset_id) {
                issues.push(format!("manifest asset_id is duplicated: {asset_id}"));
            }
            if let Err(e) = self.asset(provider, id, asset_id) {
                issues.push(format!("asset verification failed: {asset_id}: {e}"));
            }
            if source_binding {
                if !a["source_sha256"].as_str().is_some_and(sha) {
                    issues.push(format!("source sha256 is invalid: {asset_id}"));
                }
                if binding_text(a, &["source_anchor"]).is_empty() {
                    issues.push(format!("source anchor is missing: {asset_id}"));
                }
            }
        }
        Ok(issues)
    }
    pub fn validate_chapter_contract(
        &self,
        provider: &dyn SourceProvider,
        volume: &str,
        chapter: &str,
    ) -> Result<Vec<String>> {
        let id = self.chapter_package_id(volume, chapter)?;
        let p = &self.packages[id];
        let c = self.contract(provider, id)?;
        let mut issues = Vec::new();
        if c["package_id"] != id {
            issues.push("contract package_id differs from registry".into());
        }
        if c["package_version"] != p.descriptor["version"] {
            issues.push("contract version differs from registry".into());
        }
        let statuses = self.policy["status_vocabulary"]
            .as_array()
            .context("book_policy_status_missing")?;
        for (field, required_key, status_nested) in [
            ("payload_status", "required_migration_payload_kinds", false),
            ("lifecycle", "required_lifecycle_stages", true),
        ] {
            if !c[field].is_object() {
                issues.push(format!("{field} is missing"));
                continue;
            }
            for key in self.policy[required_key]
                .as_array()
                .context("book_policy_requirements_missing")?
            {
                let key = key.as_str().context("book_policy_key_invalid")?;
                let value = &c[field][key];
                if value.is_null() || (status_nested && !value.is_object()) {
                    issues.push(format!("{field} is missing {key}"));
                } else if !statuses.contains(if status_nested {
                    &value["status"]
                } else {
                    value
                }) {
                    issues.push(format!("{field} {key} is invalid"));
                }
            }
        }
        let required = self.policy["required_receipt_fields"]
            .as_array()
            .context("book_policy_receipt_missing")?;
        if !c["receipt"].is_object() {
            issues.push("receipt declaration is missing".into());
        } else {
            let wanted: BTreeSet<_> = required.iter().map(Value::to_string).collect();
            let actual: BTreeSet<_> = c["receipt"]["required_fields"]
                .as_array()
                .map_or_else(BTreeSet::new, |a| a.iter().map(Value::to_string).collect());
            if wanted != actual {
                issues.push("receipt.required_fields differs from policy".into());
            }
            if !c["receipt"]["fields"].is_object() {
                issues.push("receipt.fields is missing".into());
            } else {
                for k in required {
                    let k = k.as_str().context("book_policy_receipt_key_invalid")?;
                    if c["receipt"]["fields"].get(k).is_none() {
                        issues.push(format!("receipt.fields is missing {k}"));
                    }
                }
            }
        }
        issues.extend(self.asset_issues(provider, id, true)?);
        if c["release"]["status"] == "complete" {
            let mut incomplete = Vec::new();
            if let Some(p) = c["payload_status"].as_object() {
                incomplete.extend(
                    p.iter()
                        .filter(|(_, v)| *v != "native")
                        .map(|(k, _)| k.clone()),
                );
            }
            if let Some(p) = c["lifecycle"].as_object() {
                incomplete.extend(
                    p.iter()
                        .filter(|(_, v)| v["status"] != "native")
                        .map(|(k, _)| k.clone()),
                );
            }
            incomplete.sort();
            if !incomplete.is_empty() {
                issues.push(format!(
                    "release is complete while non-native stages remain: {}",
                    incomplete.join(", ")
                ));
            }
        }
        Ok(issues)
    }
    pub fn validate_chapter_registry(
        &self,
        provider: &dyn SourceProvider,
        require_complete: bool,
    ) -> Result<Vec<String>> {
        let mut issues = Vec::new();
        let expected = self.registry["expected_package_count"]
            .as_u64()
            .or(self.policy["contract_count"].as_u64())
            .unwrap_or(0);
        if require_complete && expected != self.chapters.len() as u64 {
            issues.push(format!(
                "registry has {} packages; expected {expected}",
                self.chapters.len()
            ));
        }
        if require_complete && let Some(coverage) = self.registry["expected_coverage"].as_object() {
            let mut expected = BTreeSet::new();
            for (v, chs) in coverage {
                for ch in chs.as_array().context("book_coverage_not_list")? {
                    expected.insert((
                        v.clone(),
                        ch.as_str().context("book_coverage_key_invalid")?.to_owned(),
                    ));
                }
            }
            let actual: BTreeSet<_> = self.chapters.keys().cloned().collect();
            let missing: Vec<_> = expected.difference(&actual).collect();
            let extra: Vec<_> = actual.difference(&expected).collect();
            if !missing.is_empty() {
                issues.push(format!("registry coverage is missing {missing:?}"));
            }
            if !extra.is_empty() {
                issues.push(format!("registry coverage has extras {extra:?}"));
            }
        }
        for ((v, ch), _) in &self.chapters {
            match self.validate_chapter_contract(provider, v, ch) {
                Ok(found) => issues.extend(found.into_iter().map(|i| format!("{v}.{ch}: {i}"))),
                Err(e) => issues.push(format!("{v}.{ch}: {e}")),
            }
        }
        for id in &self.domains {
            issues.extend(
                self.validate_domain_package(provider, id)?
                    .into_iter()
                    .map(|i| format!("{id}: {i}")),
            );
        }
        Ok(issues)
    }
}

#[derive(Clone, Copy)]
pub enum MigrationEncoding {
    LegacyAssets,
    SuccessorMappings,
}
fn binding_text<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|k| value[*k].as_str().filter(|s| !s.is_empty()))
        .unwrap_or("")
}
fn digest_check(
    provider: &dyn SourceProvider,
    check: &str,
    id: &str,
    anchor: &str,
    expected: &str,
) -> Value {
    let mut actual = None;
    let reason = if anchor.is_empty() {
        Some("source anchor is missing".into())
    } else if !sha(expected) {
        Some("expected source SHA-256 is missing or invalid".into())
    } else {
        match provider.read(anchor) {
            Ok(bytes) => {
                let h = crate::sha256(bytes);
                let reason =
                    (h != expected).then(|| "bound book source SHA-256 differs".to_owned());
                actual = Some(h);
                reason
            }
            Err(e) => Some(format!("bound book source is missing or unreadable: {e}")),
        }
    };
    json!({"actual_sha256":actual,"check_id":check,"expected_sha256":expected,"package_id":id,"reason":reason,"source_anchor":anchor,"status":if reason.is_none(){"passed"}else{"blocked"}})
}
fn binding_check(
    check: &str,
    id: &str,
    anchor: &str,
    expected: &str,
    declared_anchor: &str,
    declared_sha: &str,
) -> Value {
    let mut reasons = Vec::new();
    if anchor != declared_anchor {
        reasons.push("source anchor differs");
    }
    if expected != declared_sha {
        reasons.push("source SHA-256 differs");
    }
    json!({"actual_sha256":if declared_sha.is_empty(){None}else{Some(declared_sha)},"check_id":check,"expected_sha256":expected,"package_id":id,"reason":if reasons.is_empty(){None}else{Some(reasons.join("; "))},"source_anchor":declared_anchor,"status":if reasons.is_empty(){"passed"}else{"blocked"}})
}

/// Exact source identifiers are injected. The standard-specific algorithm does
/// not discover files, select a user's standard edition, or authorize output.
pub struct NormativeInput {
    pub package_id: String,
    pub extracts: BTreeMap<u8, String>,
    pub glossary_source: Option<String>,
    pub parts: Vec<u8>,
}
pub struct PreparedEngraving {
    pub artifacts: BTreeMap<String, Vec<u8>>,
    pub result: Value,
}

pub fn parse_teaching_cases(bytes: &[u8]) -> Result<Value> {
    let source = std::str::from_utf8(bytes).context("book_case_seed_not_utf8")?;
    let identifier = Regex::new(r"^([a-z0-9-]+):\s*$")?;
    let field = Regex::new(r"^\s{2}(ch|teaches|summary):\s*(.*)$")?;
    let mut cases = serde_json::Map::new();
    let mut current = None;
    for (n, line) in source.lines().enumerate() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(c) = identifier.captures(line) {
            let id = c[1].to_owned();
            ensure!(!cases.contains_key(&id), "book_case_duplicate_id");
            cases.insert(id.clone(), json!({}));
            current = Some(id);
            continue;
        }
        if let Some(f) = field.captures(line)
            && let Some(id) = &current
        {
            let key = &f[1];
            let raw = f[2].trim();
            let entry = cases.get_mut(id).unwrap().as_object_mut().unwrap();
            ensure!(
                !entry.contains_key(key),
                "book_case_duplicate_field:{}",
                n + 1
            );
            let v = if key == "teaches" {
                ensure!(
                    raw.starts_with('[') && raw.ends_with(']'),
                    "book_case_teaches_not_list"
                );
                json!(
                    raw[1..raw.len() - 1]
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                )
            } else {
                json!(raw)
            };
            entry.insert(key.into(), v);
            continue;
        }
        bail!("book_case_unsupported_syntax:{}", n + 1);
    }
    let result = Value::Object(cases);
    validate_cases(&result)?;
    Ok(result)
}
fn validate_cases(cases: &Value) -> Result<()> {
    let id = Regex::new(r"^[a-z0-9-]+$")?;
    for (key, c) in cases.as_object().context("book_cases_not_mapping")? {
        ensure!(id.is_match(key), "book_case_id_invalid");
        text(c, "ch")?;
        ensure!(
            !text(c, "summary")?.trim().is_empty(),
            "book_case_summary_empty"
        );
        ensure!(!text(c, "ch")?.trim().is_empty(), "book_case_ch_empty");
        ensure!(
            c["teaches"]
                .as_array()
                .is_some_and(|a| !a.is_empty()
                    && a.iter().all(|v| v.as_str().is_some_and(|s| !s.is_empty()))),
            "book_case_teaches_invalid"
        );
    }
    Ok(())
}
fn validate_glosses(glosses: &Value) -> Result<()> {
    let id = Regex::new(r"^\d+(?:\.\d+)+$")?;
    for (key, g) in glosses.as_object().context("book_glosses_not_mapping")? {
        ensure!(id.is_match(key), "book_gloss_clause_invalid");
        ensure!(!text(g, "zh")?.trim().is_empty(), "book_gloss_zh_empty");
        for key in ["keywords", "book"] {
            if let Some(value) = g.get(key) {
                ensure!(
                    value
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_string)),
                    "book_gloss_list_invalid:{key}"
                );
            }
        }
    }
    Ok(())
}
fn literal(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization is infallible")
}
pub(crate) fn canonical(value: &Value) -> Result<Vec<u8>> {
    // Feature unification may enable serde_json/preserve_order elsewhere.
    // Canonical identity must therefore sort every object explicitly.
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(object) => {
                let mut keys = object.keys().collect::<Vec<_>>();
                keys.sort();
                let mut map = serde_json::Map::new();
                for key in keys {
                    map.insert(key.clone(), sorted(&object[key]));
                }
                Value::Object(map)
            }
            Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    Ok(serde_json::to_vec(&sorted(value))?)
}
fn strings(value: &Value) -> String {
    value.as_array().map_or_else(String::new, |a| {
        a.iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("、")
    })
}
fn prefix(namespace: &str) -> String {
    format!(
        "@prefix isoN: <{namespace}> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n"
    )
}

#[derive(Clone)]
struct Block {
    page: usize,
    index: usize,
    text: String,
}
#[derive(serde::Deserialize)]
struct ExtractBlock {
    #[serde(default)]
    content: Option<OrderedContent>,
}
struct OrderedContent(Vec<Value>);
impl<'de> serde::Deserialize<'de> for OrderedContent {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct ContentVisitor;
        impl<'de> serde::de::Visitor<'de> for ContentVisitor {
            type Value = OrderedContent;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("structured extract content mapping")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> std::result::Result<Self::Value, M::Error> {
                let mut values = Vec::new();
                while let Some((_, value)) = map.next_entry::<String, Value>()? {
                    values.push(value);
                }
                Ok(OrderedContent(values))
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                value: bool,
            ) -> std::result::Result<Self::Value, E> {
                if !value {
                    Ok(OrderedContent(Vec::new()))
                } else {
                    Err(E::custom("book_extract_content_not_mapping"))
                }
            }
            fn visit_u64<E: serde::de::Error>(
                self,
                value: u64,
            ) -> std::result::Result<Self::Value, E> {
                if value == 0 {
                    Ok(OrderedContent(Vec::new()))
                } else {
                    Err(E::custom("book_extract_content_not_mapping"))
                }
            }
            fn visit_i64<E: serde::de::Error>(
                self,
                value: i64,
            ) -> std::result::Result<Self::Value, E> {
                if value == 0 {
                    Ok(OrderedContent(Vec::new()))
                } else {
                    Err(E::custom("book_extract_content_not_mapping"))
                }
            }
            fn visit_f64<E: serde::de::Error>(
                self,
                value: f64,
            ) -> std::result::Result<Self::Value, E> {
                if value == 0.0 {
                    Ok(OrderedContent(Vec::new()))
                } else {
                    Err(E::custom("book_extract_content_not_mapping"))
                }
            }
            fn visit_str<E: serde::de::Error>(
                self,
                value: &str,
            ) -> std::result::Result<Self::Value, E> {
                if value.is_empty() {
                    Ok(OrderedContent(Vec::new()))
                } else {
                    Err(E::custom("book_extract_content_not_mapping"))
                }
            }
            fn visit_seq<S: serde::de::SeqAccess<'de>>(
                self,
                mut seq: S,
            ) -> std::result::Result<Self::Value, S::Error> {
                if seq.next_element::<Value>()?.is_none() {
                    Ok(OrderedContent(Vec::new()))
                } else {
                    Err(serde::de::Error::custom("book_extract_content_not_mapping"))
                }
            }
        }
        deserializer.deserialize_any(ContentVisitor)
    }
}
fn extract_blocks(bytes: &[u8]) -> Result<Vec<Block>> {
    // Preserve original JSON member order in content.values(); sorting members
    // can move an incidental phrase before the actual clause heading.
    let value: Vec<Vec<ExtractBlock>> =
        serde_json::from_slice(bytes).context("book_extract_not_structured_json")?;
    let mut blocks = Vec::new();
    for (page, rows) in value.iter().enumerate() {
        for (index, block) in rows.iter().enumerate() {
            let Some(content) = &block.content else {
                continue;
            };
            let mut pieces = Vec::new();
            for rows in &content.0 {
                if let Some(rows) = rows.as_array() {
                    for item in rows {
                        if let Some(v) = item.get("content").filter(|v| !v.is_null()) {
                            pieces.push(if let Some(s) = v.as_str() {
                                s.to_owned()
                            } else {
                                python_string(v)
                            });
                        }
                    }
                }
            }
            let text = pieces.join(" ").trim().to_owned();
            if !text.is_empty() {
                blocks.push(Block { page, index, text });
            }
        }
    }
    ensure!(!blocks.is_empty(), "book_extract_empty");
    Ok(blocks)
}
// Python's str scalar conversion is part of the legacy structured extract contract.
fn python_string(value: &Value) -> String {
    match value {
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        Value::String(s) => s.clone(),
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(python_repr).collect::<Vec<_>>().join(", ")
        ),
        Value::Object(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, v)| format!("{}: {}", python_repr(&json!(k)), python_repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        other => other.to_string(),
    }
}
fn python_repr(v: &Value) -> String {
    if let Value::String(s) = v {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    } else {
        python_string(v)
    }
}
fn cases_ttl(namespace: &str, cases: &Value) -> Result<Vec<u8>> {
    let mut lines = vec![
        prefix(namespace),
        "# Released synthetic teaching-case layer.\n".into(),
    ];
    let ordered: BTreeMap<_, _> = cases
        .as_object()
        .context("book_cases_not_mapping")?
        .iter()
        .collect();
    for (id, c) in ordered {
        lines.push(format!(
            "isoN:Case_{} a isoN:TeachingCase ;",
            id.replace('-', "_")
        ));
        lines.push(format!("    isoN:inChapter {} ;", literal(text(c, "ch")?)));
        lines.push(format!(
            "    isoN:teachesConcepts {} ;",
            literal(&strings(&c["teaches"]))
        ));
        lines.push(format!(
            "    isoN:caseSummary {} .\n",
            literal(text(c, "summary")?)
        ));
    }
    Ok((lines.join("\n") + "\n").into_bytes())
}
fn link_case(name: &str, gloss: &str, cases: &Value) -> Option<String> {
    let mut best = None;
    let mut score = 0;
    let ordered: BTreeMap<_, _> = cases.as_object()?.iter().collect();
    for (id, c) in ordered {
        let candidate = c["teaches"]
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(|k| {
                if name.contains(k) {
                    3 * k.chars().count()
                } else if gloss.contains(k) {
                    k.chars().count()
                } else {
                    0
                }
            })
            .sum();
        if candidate > score {
            best = Some(id.clone());
            score = candidate;
        }
    }
    if score >= 4 { best } else { None }
}
fn part1(
    namespace: &str,
    glossary: &str,
    blocks: &[Block],
    cases: &Value,
) -> Result<(BTreeMap<String, Vec<u8>>, Value)> {
    let pattern = Regex::new(
        r"(?m)^- \*\*(?P<zh>[^（*]+)（(?P<en>[^）]+)）\*\*（1-3\.(?P<num>\d+)(?P<meta>[^）]*)）——(?P<gloss>.+)$",
    )?;
    let chapters = Regex::new(r"ch\d+")?;
    let anchor = Regex::new(r"^3\.(\d+)(?:\s|$)")?;
    let mut units = BTreeMap::new();
    let mut index = BTreeMap::new();
    for b in blocks {
        if let Some(c) = anchor.captures(&b.text) {
            let n = c[1].parse::<u64>()?;
            let coords = (b.page, b.index);
            if let Some(old) = index.insert(n, coords) {
                ensure!(old == coords, "book_part1_ambiguous_anchor");
            }
        }
    }
    for c in pattern.captures_iter(glossary) {
        let n = c["num"].parse::<u64>()?;
        let coords = *index.get(&n).context("book_part1_anchor_missing")?;
        let zh = c["zh"].trim().to_owned();
        let en = c["en"].trim().to_owned();
        let gloss = c["gloss"].replace("**", "").trim().to_owned();
        let serves = chapters
            .find_iter(&c["meta"])
            .map(|m| m.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let case = link_case(&format!("{zh} {en}"), &gloss, cases);
        ensure!(
            units
                .insert(n, (zh, en, gloss, serves, coords, case))
                .is_none(),
            "book_part1_duplicate_term"
        );
    }
    ensure!(!units.is_empty(), "book_part1_glossary_empty");
    let first = *units.first_key_value().unwrap().0;
    let last = *units.last_key_value().unwrap().0;
    let mut ttl = vec![
        prefix(namespace),
        "# Part 1 terms: author glosses plus controlled-source coordinates; no standard text.\n"
            .into(),
    ];
    let mut cards = vec![
        "# ISO 26262 Part 1 术语卡（本体化刻录 · 卡片视图）\n".into(),
        "> 自动生成；转述来自外部书侧作者内容，非标准原文。\n".into(),
        format!(
            "> **覆盖声明**：本层收录词条 1-3.{first} 至 1-3.{last} 共 {} 条；范围外须回受控标准来源核对。\n",
            units.len()
        ),
    ];
    for (n, (zh, en, gloss, serves, (p, b), case)) in &units {
        ttl.push(format!("isoN:P1_T{n} a isoN:TermDefinition ;"));
        ttl.push(format!(
            "    isoN:clauseId {} ; isoN:partNumber 1 ;",
            literal(&format!("1-3.{n}"))
        ));
        ttl.push(format!(
            "    rdfs:label {}@zh ; isoN:enLabel {} ;",
            literal(zh),
            literal(en)
        ));
        ttl.push("    isoN:modality isoN:Definition ; isoN:glossStatus \"glossed\" ;".into());
        if !serves.is_empty() {
            ttl.push(format!("    isoN:servesChapter {} ;", literal(serves)));
        }
        ttl.push(format!("    isoN:pageIndex {p} ; isoN:blockIndex {b} ;"));
        if let Some(case) = case {
            ttl.push(format!(
                "    isoN:taughtBy isoN:Case_{} ;",
                case.replace('-', "_")
            ));
        }
        ttl.push(format!("    isoN:zhGloss {} .\n", literal(gloss)));
        cards.push(format!("### 1-3.{n} {zh}（{en}）｜术语"));
        cards.push(format!("转述：{gloss}"));
        if let Some(case) = case {
            let c = &cases[case];
            cards.push(format!(
                "书中讲法：[{}] {}",
                text(c, "ch")?,
                text(c, "summary")?
            ));
        }
        cards.push(format!(
            "映射：{} ｜ 提取件锚点：p{p}/b{b}\n",
            if serves.is_empty() { "—" } else { serves }
        ));
    }
    ttl.extend([
        "isoN:Coverage_P1 a isoN:CoverageDeclaration ; isoN:partNumber 1 ;".into(),
        format!(
            "    isoN:coversRange {} ; isoN:unitCount {} ;",
            literal(&format!("1-3.{first} 至 1-3.{last}")),
            units.len()
        ),
        format!(
            "    rdfs:comment {}@zh .",
            literal("本层只声明已刻录范围；范围外须回受控标准来源核对。")
        ),
    ]);
    Ok((
        BTreeMap::from([
            (
                "part1-vocabulary.ttl".into(),
                (ttl.join("\n") + "\n").into_bytes(),
            ),
            (
                "part1-cards.md".into(),
                (cards.join("\n") + "\n").into_bytes(),
            ),
        ]),
        json!({"part":1,"unit_count":units.len(),"glossed_count":units.len(),"pending_count":0}),
    ))
}
fn modality(text: &str) -> Result<&'static str> {
    let text = text.trim();
    let upper = text.to_uppercase();
    if upper.starts_with("NOTE") {
        return Ok("Note");
    }
    if upper.starts_with("EXAMPLE") {
        return Ok("Example");
    }
    let lowered = format!(" {} ", text.to_lowercase());
    for (word, value) in [("shall", "Shall"), ("should", "Should"), ("may", "May")] {
        if Regex::new(&format!(r"\b{word}\b"))?.is_match(&lowered) {
            return Ok(value);
        }
    }
    Ok(if lowered.contains(" can be ") {
        "May"
    } else {
        "Structural"
    })
}
fn part3(
    namespace: &str,
    blocks: &[Block],
    glosses: &Value,
) -> Result<(BTreeMap<String, Vec<u8>>, Value)> {
    let pattern = Regex::new(r"^(\d+(?:\.\d+)+)[\s\t]+(\S.*)$")?;
    let mut units = BTreeMap::new();
    for b in blocks {
        if let Some(c) = pattern.captures(&b.text) {
            let id = c[1].to_owned();
            let key = id
                .split('.')
                .map(str::parse::<u64>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let candidate = (id, b.page, b.index, modality(&b.text)?);
            if let Some(old) = units.insert(key, candidate.clone()) {
                ensure!(old == candidate, "book_part3_ambiguous_heading");
            }
        }
    }
    ensure!(!units.is_empty(), "book_part3_headings_missing");
    let mut ttl=vec![prefix(namespace),"# Part 3 skeleton: controlled-source coordinates/modalities plus released gloss seeds; no standard text.\n".into()];
    let mut cards = vec![
        "# ISO 26262 Part 3 条款卡（本体化刻录 · 卡片视图）\n".into(),
        "> 骨架来自受控提取件坐标；转述来自内建公开 seed；不含标准原文。\n".into(),
    ];
    let mut glossed = 0;
    for (id, p, b, mode) in units.values() {
        ttl.push(format!(
            "isoN:P3_{} a isoN:NormativeUnit ;",
            id.replace('.', "_")
        ));
        ttl.push(format!(
            "    isoN:clauseId {} ; isoN:partNumber 3 ;",
            literal(&format!("3-{id}"))
        ));
        ttl.push(format!("    isoN:modality isoN:{mode} ;"));
        ttl.push(format!("    isoN:pageIndex {p} ; isoN:blockIndex {b} ;"));
        let Some(gloss) = glosses.get(id) else {
            ttl.push("    isoN:glossStatus \"pending\" .\n".into());
            continue;
        };
        glossed += 1;
        let keywords = strings(&gloss["keywords"]);
        let book = strings(&gloss["book"]);
        ttl.push(format!(
            "    isoN:zhGloss {} ;",
            literal(text(gloss, "zh")?)
        ));
        if !keywords.is_empty() {
            ttl.push(format!("    isoN:keywords {} ;", literal(&keywords)));
        }
        if !book.is_empty() {
            ttl.push(format!("    isoN:servesChapter {} ;", literal(&book)));
        }
        ttl.push("    isoN:glossStatus \"glossed\" .\n".into());
        cards.push(format!("### 3-{id}｜{mode}｜glossed"));
        cards.push(format!("转述：{}", text(gloss, "zh")?));
        cards.push(format!(
            "关键词：{} ｜ 映射：{} ｜ 锚点：p{p}/b{b}\n",
            if keywords.is_empty() {
                "—"
            } else {
                &keywords
            },
            if book.is_empty() { "—" } else { &book }
        ));
    }
    let pending = units.len() - glossed;
    cards.push(format!(
        "---\n骨架总数：{} ｜ 已刻转述：{glossed} ｜ 待刻：{pending}",
        units.len()
    ));
    Ok((
        BTreeMap::from([
            (
                "part3-concept-phase.ttl".into(),
                (ttl.join("\n") + "\n").into_bytes(),
            ),
            (
                "part3-cards.md".into(),
                (cards.join("\n") + "\n").into_bytes(),
            ),
        ]),
        json!({"part":3,"unit_count":units.len(),"glossed_count":glossed,"pending_count":pending}),
    ))
}

pub fn prepare_normative_engraving(
    catalog: &Catalog,
    packages: &dyn SourceProvider,
    sources: &dyn SourceProvider,
    input: &NormativeInput,
) -> Result<PreparedEngraving> {
    let mut parts = input.parts.clone();
    ensure!(!parts.is_empty(), "book_engrave_parts_empty");
    parts.sort();
    ensure!(
        parts.iter().all(|p| matches!(p, 1 | 3)) && !parts.windows(2).any(|w| w[0] == w[1]),
        "book_engrave_part_unsupported_or_duplicate"
    );
    let manifest = catalog.read_manifest(&input.package_id)?;
    catalog.load_package(
        packages,
        &input.package_id,
        &mut crate::dataset::DatasetSession::new(),
    )?;
    let namespace = text(manifest, "namespace")?;
    // Validate an injected namespace as an actual RDF IRI before interpolating it.
    oxigraph::model::NamedNode::new(namespace)?;
    let tbox = catalog.asset(packages, &input.package_id, "normative-tbox")?;
    let gloss_bytes = catalog.asset(packages, &input.package_id, "gloss-part3-glosses")?;
    let case_bytes = catalog.asset(packages, &input.package_id, "gloss-teaching-cases")?;
    let glosses = packages.metadata(&gloss_bytes)?;
    validate_glosses(&glosses)?;
    let cases = parse_teaching_cases(&case_bytes)?;
    let mut hashes = BTreeMap::from([
        ("seed.normative-tbox".to_owned(), crate::sha256(&tbox)),
        ("seed.part3-glosses".to_owned(), crate::sha256(&gloss_bytes)),
        ("seed.teaching-cases".to_owned(), crate::sha256(&case_bytes)),
    ]);
    let mut artifacts = BTreeMap::from([
        ("normative-tbox.ttl".to_owned(), tbox),
        (
            "teaching-cases.ttl".to_owned(),
            cases_ttl(namespace, &cases)?,
        ),
    ]);
    let mut results = Vec::new();
    for part in parts {
        let id = input
            .extracts
            .get(&part)
            .context("book_engrave_extract_id_missing")?;
        let raw = sources.read(id)?;
        ensure!(raw.len() <= crate::MAX_BYTES, "book_extract_budget");
        let blocks = extract_blocks(&raw)?;
        hashes.insert(format!("controlled.part{part}.extract"), crate::sha256(raw));
        let (files, result) = if part == 1 {
            let id = input
                .glossary_source
                .as_ref()
                .context("book_engrave_glossary_id_missing")?;
            let raw = sources.read(id)?;
            ensure!(raw.len() <= crate::MAX_BYTES, "book_glossary_budget");
            hashes.insert("book.appendix-c-glossary".into(), crate::sha256(&raw));
            part1(namespace, std::str::from_utf8(&raw)?, &blocks, &cases)?
        } else {
            part3(namespace, &blocks, &glosses)?
        };
        artifacts.extend(files);
        results.push(result);
    }
    let rdf: Vec<_> = artifacts
        .iter()
        .filter(|(name, _)| name.ends_with(".ttl"))
        .map(|(_, data)| (data.as_slice(), oxigraph::io::RdfFormat::Turtle))
        .collect();
    let dataset = crate::graph::load(&crate::graph::merge_sources(&rdf)?)?;
    let snapshot = crate::dataset::snapshot(&dataset, 0)?;
    let dtos = artifact_dtos(&artifacts);
    let package_digest = package_digest(manifest)?;
    let content = json!({"schema_version":"1.0","package_id":input.package_id,"package_version":manifest["version"],"package_digest":package_digest,"parts":results,"source_hashes":hashes,"artifacts":dtos,"semantic_dataset_sha256":snapshot.dataset_sha256,"semantic_dataset_hash_profile":snapshot.canonicalization,"restricted_standard_text_included":false});
    let bundle = crate::sha256(canonical(&content)?);
    let mut result = content;
    result["bundle_sha256"] = json!(bundle);
    let mut report = canonical(&result)?;
    report.push(b'\n');
    artifacts.insert("engraving-report.json".into(), report);
    result["artifacts"] = json!(artifact_dtos(&artifacts));
    Ok(PreparedEngraving { artifacts, result })
}
pub fn artifact_dtos(artifacts: &BTreeMap<String, Vec<u8>>) -> Vec<Value> {
    artifacts.iter().map(|(name,bytes)|json!({"relative_path":name,"media_type":if name.ends_with(".ttl"){"text/turtle"}else if name.ends_with(".json"){"application/json"}else if name.ends_with(".md"){"text/markdown"}else{"application/octet-stream"},"sha256":crate::sha256(bytes),"size_bytes":bytes.len()})).collect()
}
/// Legacy identity serializer shared with generic actual-byte package preparation.
pub fn package_digest(manifest: &Value) -> Result<String> {
    crate::packages::package_digest(manifest)
}
pub fn package_identity_bytes(manifest: &Value) -> Result<Vec<u8>> {
    crate::packages::package_identity_bytes(manifest)
}
