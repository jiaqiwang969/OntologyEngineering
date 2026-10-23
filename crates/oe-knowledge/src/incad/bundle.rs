//! Transport selected inCAD source files without changing their upstream identity.
//! The bundle verifies bytes and paths, not the source graph or engineering use.
use crate::{publication, source_pack};
use anyhow::{Context, Result, ensure};
use oe_core::{SkillRoot, json::strict_json, sha256};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const SCHEMA: &str = "ontology-engineering.incad-source-bundle/v1";
const SHARD_SCHEMA: &str = "ontology-engineering.incad-source-shard/v1";
const MAX_MANIFEST: usize = 1024 * 1024;
const MAX_DESCRIPTOR: usize = 16 * 1024 * 1024;
const MAX_DESCRIPTORS_TOTAL: u64 = 128 * 1024 * 1024;
const MAX_SHARDS: usize = 256;
const MAX_SHARD_MEMBERS: usize = 16_384;
const MAX_MEMBERS: usize = 262_144;
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SHARD_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const MAX_IMPORT_WORKERS: usize = 2;
// Parallel workers are useful for the real, archive-sized shards, but add
// filesystem and decoder contention for tiny shards. Keep the latter on the
// already verified serial path. This is based only on declared member bytes;
// the complete descriptor/path preflight still runs before this decision.
const MIN_PARALLEL_SHARD_BYTES: u64 = 64 * 1024 * 1024;
const EXCLUDED: [&str; 5] = [
    "browser_capture",
    "captures",
    "jobs",
    "runtime",
    "private_projects",
];

fn closed(value: &Value, keys: &[&str]) -> Result<()> {
    let object = value.as_object().context("incad_bundle_object_required")?;
    ensure!(
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key)),
        "incad_bundle_unknown_or_missing_field"
    );
    Ok(())
}

fn relative(value: &str) -> Result<&Path> {
    ensure!(value.chars().count() <= 2000, "incad_bundle_path_too_long");
    source_pack::relative_path(value).context("incad_bundle_invalid_relative_path")
}

/// Only the published source layout can enter a restored tree. This is a path
/// policy, not a claim that arbitrary HTML/free text has been privacy-reviewed.
fn deliverable(path: &str) -> bool {
    if path.split('/').any(|part| {
        part.eq_ignore_ascii_case("browser-capture.json")
            || EXCLUDED
                .iter()
                .any(|category| part.eq_ignore_ascii_case(category))
    }) {
        return false;
    }
    path.starts_with("cases/cn/")
        || path.starts_with("design-inputs/cn/")
        || path == "design-inputs/index.json"
        || path.starts_with("catalog/cn/")
        || path.starts_with("navigation/assets/")
        || path.starts_with("navigation/vendor/")
        || path.starts_with("navigation/taxonomy/")
        || matches!(
            path,
            "index.html"
                | "overview.svg"
                | "status.json"
                | "navigation/index.html"
                | "navigation/data.js"
                | "navigation/catalog.json"
                | "navigation/taxonomy.json"
                | "navigation/official-layout.json"
                | "navigation/status.json"
        )
}

#[derive(Clone)]
struct Bound {
    path: String,
    sha256: String,
    bytes: u64,
}

impl Bound {
    fn parse(value: &Value, limit: u64) -> Result<Self> {
        closed(value, &["path", "sha256", "bytes"])?;
        Self::fields(value, limit)
    }

    fn fields(value: &Value, limit: u64) -> Result<Self> {
        let path = value["path"]
            .as_str()
            .context("incad_bundle_path_required")?;
        relative(path)?;
        let hash = value["sha256"]
            .as_str()
            .context("incad_bundle_sha_required")?;
        let bytes = value["bytes"]
            .as_u64()
            .context("incad_bundle_bytes_required")?;
        ensure!(
            source_pack::valid_sha(hash) && bytes <= limit,
            "incad_bundle_identity_invalid"
        );
        Ok(Self {
            path: path.into(),
            sha256: hash.into(),
            bytes,
        })
    }

    fn value(&self) -> Value {
        json!({"path": self.path, "sha256": self.sha256, "bytes": self.bytes})
    }

    fn exact(&self, path: &Path) -> Result<()> {
        ensure!(
            source_pack::file_hash(path, self.bytes)? == (self.sha256.clone(), self.bytes),
            "incad_bundle_file_changed"
        );
        Ok(())
    }
}

struct Part {
    descriptor: Bound,
    descriptor_raw: Vec<u8>,
    archive: Bound,
    files: Vec<Bound>,
}

fn parallel_import_eligible(parts: &[Part]) -> bool {
    parts.len() > 1
        && parts.iter().all(|part| {
            part.files
                .iter()
                .try_fold(0_u64, |total, file| total.checked_add(file.bytes))
                .is_some_and(|bytes| bytes >= MIN_PARALLEL_SHARD_BYTES)
        })
}

impl Part {
    fn archive_value(&self) -> Value {
        let mut value = self.archive.value();
        value["files"] = json!(self.files.iter().map(Bound::value).collect::<Vec<_>>());
        value
    }
}

/// A batch owns every worker until it has joined. Keep manifest order when
/// choosing an error, rather than letting completion order select the result.
/// No later batch starts after any worker, spawn or panic failure.
fn import_batches<T: Sync>(parts: &[T], visit: impl Fn(&T) -> Result<()> + Sync) -> Result<()> {
    for batch in parts.chunks(MAX_IMPORT_WORKERS) {
        std::thread::scope(|scope| {
            let mut workers = Vec::with_capacity(batch.len());
            let mut spawn_error = None;
            for part in batch {
                let visit = &visit;
                match std::thread::Builder::new()
                    .name("incad-bundle-import".into())
                    .spawn_scoped(scope, move || visit(part))
                {
                    Ok(worker) => workers.push(worker),
                    Err(error) => {
                        spawn_error = Some(
                            anyhow::Error::new(error).context("incad_bundle_worker_spawn_failed"),
                        );
                        break;
                    }
                }
            }
            let mut first_error = None;
            for worker in workers {
                let result = match worker.join() {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!("incad_bundle_worker_panicked")),
                };
                if let Err(error) = result
                    && first_error.is_none()
                {
                    first_error = Some(error);
                }
            }
            // Spawn failure belongs after all workers that were actually
            // started. Preserve an earlier worker's original error and context.
            match first_error.or(spawn_error) {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })?;
    }
    Ok(())
}

struct Bundle {
    manifest: PathBuf,
    raw: Vec<u8>,
    value: Value,
    parts: Vec<Part>,
    member_count: usize,
    uncompressed_bytes: u64,
    compressed_bytes: u64,
}

impl Bundle {
    fn base(&self) -> &Path {
        self.manifest.parent().expect("manifest has parent")
    }

    fn load(root: &SkillRoot, manifest: &Path) -> Result<Self> {
        let manifest = source_pack::within_root(root.path(), manifest)?;
        let raw = oe_core::read_regular_bytes(&manifest, MAX_MANIFEST)?;
        let value = strict_json(&raw)?;
        closed(
            &value,
            &["schema", "bundle_id", "case_count", "scope", "shards"],
        )?;
        let contract: oe_contracts::IncadSourceBundleManifest =
            serde_json::from_value(value.clone())?;
        contract
            .validate()
            .map_err(|_| anyhow::anyhow!("incad_bundle_generated_contract_invalid"))?;
        ensure!(value["schema"] == SCHEMA, "incad_bundle_schema_mismatch");
        let id = value["bundle_id"]
            .as_str()
            .context("incad_bundle_id_required")?;
        ensure!(
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
            "incad_bundle_id_invalid"
        );
        let case_count = value["case_count"]
            .as_u64()
            .context("incad_bundle_case_count_invalid")?;
        ensure!(
            (1..=4096).contains(&case_count),
            "incad_bundle_case_count_invalid"
        );
        let scope = &value["scope"];
        closed(
            scope,
            &[
                "kind",
                "full_upstream_manifest_reproduction",
                "excluded",
                "excluded_paths",
            ],
        )?;
        ensure!(
            scope["kind"] == "selected_current_case_closure"
                && scope["full_upstream_manifest_reproduction"] == false,
            "incad_bundle_scope_invalid"
        );
        let categories = scope["excluded"]
            .as_array()
            .context("incad_bundle_exclusions_invalid")?;
        let declared: BTreeSet<_> = categories.iter().filter_map(Value::as_str).collect();
        ensure!(
            categories.len() == EXCLUDED.len() && declared == EXCLUDED.into_iter().collect(),
            "incad_bundle_exclusions_invalid"
        );
        let omissions = scope["excluded_paths"]
            .as_array()
            .context("incad_bundle_exclusions_invalid")?;
        ensure!(omissions.len() <= 4096, "incad_bundle_exclusions_limit");
        let mut omitted = BTreeSet::new();
        for item in omissions {
            let path = item.as_str().context("incad_bundle_exclusions_invalid")?;
            relative(path)?;
            ensure!(
                !deliverable(path) && omitted.insert(path.to_owned()),
                "incad_bundle_exclusions_invalid"
            );
        }
        let shards = value["shards"]
            .as_array()
            .context("incad_bundle_shards_required")?;
        ensure!(
            !shards.is_empty() && shards.len() <= MAX_SHARDS,
            "incad_bundle_shard_limit"
        );
        let base = manifest.parent().context("incad_bundle_manifest_parent")?;
        let mut inputs = BTreeSet::from([manifest.clone()]);
        let mut paths = BTreeSet::new();
        let mut parts = Vec::new();
        let mut metadata_bytes = 0_u64;
        let mut uncompressed_bytes = 0_u64;
        let mut compressed_bytes = 0_u64;
        for shard in shards {
            let descriptor = Bound::parse(shard, MAX_DESCRIPTOR as u64)?;
            ensure!(
                descriptor.path.ends_with(".json") && descriptor.bytes > 0,
                "incad_bundle_descriptor_invalid"
            );
            metadata_bytes = metadata_bytes
                .checked_add(descriptor.bytes)
                .context("incad_bundle_size_overflow")?;
            ensure!(
                metadata_bytes <= MAX_DESCRIPTORS_TOTAL,
                "incad_bundle_metadata_limit"
            );
            let descriptor_path =
                source_pack::within_root(root.path(), &base.join(&descriptor.path))?;
            ensure!(
                inputs.insert(descriptor_path.clone()),
                "incad_bundle_duplicate_input"
            );
            let descriptor_raw = oe_core::read_regular_bytes(&descriptor_path, MAX_DESCRIPTOR)?;
            ensure!(
                descriptor_raw.len() as u64 == descriptor.bytes
                    && sha256(&descriptor_raw) == descriptor.sha256,
                "incad_bundle_descriptor_changed"
            );
            let part = strict_json(&descriptor_raw)?;
            closed(&part, &["schema", "path", "sha256", "bytes", "files"])?;
            ensure!(
                part["schema"] == SHARD_SCHEMA,
                "incad_bundle_shard_schema_mismatch"
            );
            let archive = Bound::fields(&part, MAX_SHARD_BYTES)?;
            ensure!(
                archive.path.ends_with(".tar.xz") && archive.bytes > 0,
                "incad_bundle_archive_invalid"
            );
            let archive_path = source_pack::within_root(root.path(), &base.join(&archive.path))?;
            ensure!(inputs.insert(archive_path), "incad_bundle_duplicate_input");
            compressed_bytes = compressed_bytes
                .checked_add(archive.bytes)
                .context("incad_bundle_size_overflow")?;
            ensure!(
                compressed_bytes <= MAX_TOTAL_BYTES,
                "incad_bundle_compressed_limit"
            );
            let members = part["files"]
                .as_array()
                .context("incad_bundle_members_required")?;
            ensure!(
                !members.is_empty() && members.len() <= MAX_SHARD_MEMBERS,
                "incad_bundle_member_limit"
            );
            let mut files = Vec::new();
            let mut part_bytes = 0_u64;
            for member in members {
                let file = Bound::parse(member, MAX_FILE_BYTES)?;
                ensure!(
                    deliverable(&file.path) && !omitted.contains(&file.path),
                    "incad_bundle_excluded_member"
                );
                ensure!(
                    paths.insert(file.path.clone()),
                    "incad_bundle_duplicate_member"
                );
                ensure!(paths.len() <= MAX_MEMBERS, "incad_bundle_member_limit");
                part_bytes = part_bytes
                    .checked_add(file.bytes)
                    .context("incad_bundle_size_overflow")?;
                ensure!(
                    part_bytes <= MAX_SHARD_BYTES,
                    "incad_bundle_shard_expansion_limit"
                );
                files.push(file);
            }
            uncompressed_bytes = uncompressed_bytes
                .checked_add(part_bytes)
                .context("incad_bundle_size_overflow")?;
            ensure!(
                uncompressed_bytes <= MAX_TOTAL_BYTES,
                "incad_bundle_expansion_limit"
            );
            parts.push(Part {
                descriptor,
                descriptor_raw,
                archive,
                files,
            });
        }
        // Resolve conflicts across every shard before creating an import stage.
        for name in &paths {
            let mut parent = Path::new(name).parent();
            while let Some(path) = parent {
                ensure!(
                    !paths.contains(path.to_str().context("incad_bundle_path_not_utf8")?),
                    "incad_bundle_file_directory_conflict"
                );
                parent = path.parent();
            }
        }
        let pointers = paths
            .iter()
            .filter(|name| {
                let parts: Vec<_> = name.split('/').collect();
                parts.len() == 4
                    && parts[0] == "cases"
                    && parts[1] == "cn"
                    && parts[2].len() == 6
                    && parts[2].bytes().all(|b| b.is_ascii_digit())
                    && parts[3] == "current.json"
            })
            .count();
        ensure!(
            pointers as u64 == case_count,
            "incad_bundle_case_inventory_mismatch"
        );
        let bundle = Self {
            manifest,
            raw,
            value,
            parts,
            member_count: paths.len(),
            uncompressed_bytes,
            compressed_bytes,
        };
        bundle.ensure_metadata()?;
        Ok(bundle)
    }

    fn ensure_metadata(&self) -> Result<()> {
        ensure!(
            oe_core::read_regular_bytes(&self.manifest, MAX_MANIFEST)? == self.raw,
            "incad_bundle_manifest_changed"
        );
        for part in &self.parts {
            ensure!(
                oe_core::read_regular_bytes(
                    &self.base().join(&part.descriptor.path),
                    MAX_DESCRIPTOR
                )? == part.descriptor_raw,
                "incad_bundle_descriptor_changed"
            );
        }
        Ok(())
    }

    fn ensure_current(&self) -> Result<()> {
        self.ensure_metadata()?;
        for part in &self.parts {
            part.archive.exact(&self.base().join(&part.archive.path))?;
        }
        self.ensure_metadata()
    }

    fn visit_part(&self, root: &SkillRoot, part: &Part, built: Option<&Path>) -> Result<()> {
        let none = BTreeSet::new();
        source_pack::visit_archive(
            root.path(),
            &self.base().join(&part.archive.path),
            &part.archive_value(),
            &source_pack::ArchiveLimits {
                max_file_bytes: MAX_FILE_BYTES,
                max_members: MAX_SHARD_MEMBERS,
                max_uncompressed_bytes: MAX_SHARD_BYTES,
            },
            built.is_none().then_some(&none),
            |member| {
                let directory = built.context("incad_bundle_unexpected_selected_member")?;
                retain_member(member, &directory.join(&member.relative_path))
            },
        )?;
        Ok(())
    }

    fn visit(&self, root: &SkillRoot, built: Option<&Path>) -> Result<()> {
        if built.is_some() && parallel_import_eligible(&self.parts) {
            import_batches(&self.parts, |part| self.visit_part(root, part, built))?;
        } else {
            for part in &self.parts {
                self.visit_part(root, part, built)?;
            }
        }
        self.ensure_current()
    }

    fn verify_staged(&self, built: &Path) -> Result<()> {
        for part in &self.parts {
            for file in &part.files {
                file.exact(&built.join(&file.path))?;
            }
        }
        Ok(())
    }

    fn report(&self, root: &SkillRoot, status: &str, target: Option<&Path>) -> Result<Value> {
        let directory = target
            .map(|path| {
                Ok::<_, anyhow::Error>(
                    path.strip_prefix(root.path())?
                        .to_str()
                        .context("incad_bundle_path_not_utf8")?
                        .to_owned(),
                )
            })
            .transpose()?;
        Ok(json!({
            "schema":"ontology-engineering.incad-source-bundle-result/v1",
            "status": status,
            "bundle_id": self.value["bundle_id"],
            "manifest": self.manifest.strip_prefix(root.path())?.to_str().context("incad_bundle_path_not_utf8")?,
            "manifest_sha256": sha256(&self.raw),
            "directory": directory,
            "case_count": self.value["case_count"],
            "shards": self.parts.len(), "files": self.member_count,
            "uncompressed_bytes": self.uncompressed_bytes, "compressed_bytes": self.compressed_bytes,
            "scope": self.value["scope"],
            "scope_validation": "declared_inventory_and_path_policy",
            "source_closure_verification":"not_run", "ontology_validation":"not_run",
            "native_cad_validation":"not_run", "engineering_adoption":"not_granted",
            "limits": {"manifest_bytes":MAX_MANIFEST,"descriptor_bytes":MAX_DESCRIPTOR,
                "descriptor_bytes_total":MAX_DESCRIPTORS_TOTAL,"shards":MAX_SHARDS,
                "members_per_shard":MAX_SHARD_MEMBERS,"members_total":MAX_MEMBERS,
                "file_bytes":MAX_FILE_BYTES,"shard_bytes":MAX_SHARD_BYTES,
                "shard_uncompressed_bytes":MAX_SHARD_BYTES,"total_uncompressed_bytes":MAX_TOTAL_BYTES,
                "total_compressed_bytes":MAX_TOTAL_BYTES},
            "meaning":"Selected file transport verified; upstream manifests retain omitted entries. No source-graph, Project, CAD or engineering approval."
        }))
    }
}

fn retain_member(member: &source_pack::VerifiedMember, destination: &Path) -> Result<()> {
    ensure!(
        !destination.try_exists()?,
        "incad_bundle_staged_member_exists"
    );
    retain_new_member(member, destination)
}

fn retain_new_member(member: &source_pack::VerifiedMember, destination: &Path) -> Result<()> {
    // The shared visitor has already streamed, hashed and synced this private
    // regular file. Retain that inode in our private stage before the visitor
    // removes its temporary name, as restore_selected already does. No archive
    // supplied links are accepted. The target is hashed here, the visitor
    // rechecks its name after this callback, and publication rechecks every
    // staged file and input. Both stages share this root's cache filesystem.
    // Unlike reusable page-cache entries, each destination must be newly
    // created. Literal paths can alias on a case-insensitive filesystem, so an
    // EEXIST race must fail even when the already linked bytes happen to match.
    source_pack::publish_file_no_replace(&member.path, destination)?;
    ensure!(
        source_pack::file_hash(destination, member.bytes)? == (member.sha256.clone(), member.bytes),
        "incad_bundle_staged_member_changed"
    );
    Ok(())
}

/// Verify every shard and member. Temporary streaming files are removed; no
/// source directory, registration, Project or durable report is created here.
pub fn verify(root: &SkillRoot, manifest: &Path) -> Result<Value> {
    let bundle = Bundle::load(root, manifest)?;
    bundle.visit(root, None)?;
    bundle.report(root, "verified", None)
}

/// Restore only into a new real directory under sources/misumi. All shards are
/// staged together; a late failure never publishes a partial target.
pub fn import(root: &SkillRoot, manifest: &Path, directory: &Path) -> Result<Value> {
    let target = source_pack::within_root(root.path(), directory)?;
    let allowed = root.path().join("sources/misumi");
    ensure!(
        target.starts_with(&allowed) && target != allowed,
        "incad_bundle_target_outside_misumi"
    );
    ensure!(
        fs::symlink_metadata(&target)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "incad_bundle_target_exists"
    );
    let bundle = Bundle::load(root, manifest)?;
    let report = bundle.report(root, "imported", Some(&target))?;
    let stage = source_pack::Stage::new(root.path(), "incad-bundle")?;
    let built = stage.path.join("archive");
    oe_core::create_new_private_dir(&built)?;
    bundle.visit(root, Some(&built))?;
    publication::publish_directory(
        stage,
        &built,
        &target,
        None,
        |_| anyhow::bail!("incad_bundle_replacement_forbidden"),
        || {
            bundle.verify_staged(&built)?;
            bundle.ensure_current()
        },
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    };
    use std::time::Duration;
    use tar::{Builder, Header};
    use xz2::write::XzEncoder;

    const POINTER: &str = "cases/cn/000210/current.json";
    const CAD: &str = "cases/cn/000210/attempts/fixed/cad/ap203/files/model.step";
    const OMITTED: &str = "cases/cn/000210/attempts/fixed/browser-capture.json";
    const TEST_DEADLINE: Duration = Duration::from_secs(10);

    struct Fixture {
        _temp: tempfile::TempDir,
        root: SkillRoot,
        parts: Vec<Value>,
    }

    fn save(path: &Path, value: &Value) -> Value {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let raw = serde_json::to_vec_pretty(value).unwrap();
        fs::write(path, &raw).unwrap();
        json!({"path":path.file_name().unwrap().to_str().unwrap(),"sha256":sha256(&raw),"bytes":raw.len()})
    }

    impl Fixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let directory = fs::canonicalize(temp.path()).unwrap();
            fs::write(directory.join("SKILL.md"), "fixture").unwrap();
            let root = SkillRoot::new(&directory).unwrap();
            fs::create_dir_all(root.path().join("bundle")).unwrap();
            Self {
                _temp: temp,
                root,
                parts: Vec::new(),
            }
        }

        fn shard(&mut self, files: &[(&str, &[u8])], link: bool) {
            let number = self.parts.len();
            let name = format!("part-{number}.tar.xz");
            let mut tar = Builder::new(XzEncoder::new(Vec::new(), 0));
            for (path, bytes) in files {
                let mut header = Header::new_gnu();
                header.set_mode(0o600);
                header.set_size(bytes.len() as u64);
                if link {
                    header.set_entry_type(tar::EntryType::Symlink);
                    header.set_link_name("../../outside").unwrap();
                }
                header.set_cksum();
                tar.append_data(&mut header, *path, *bytes).unwrap();
            }
            let raw = tar.into_inner().unwrap().finish().unwrap();
            fs::write(self.root.path().join("bundle").join(&name), &raw).unwrap();
            let part = json!({"schema":SHARD_SCHEMA,"path":name,"bytes":raw.len(),"sha256":sha256(&raw),
                "files":files.iter().map(|(path,bytes)|json!({"path":path,"bytes":bytes.len(),"sha256":sha256(bytes)})).collect::<Vec<_>>()});
            let descriptor = self.root.path().join(format!("bundle/part-{number}.json"));
            self.parts.push(save(&descriptor, &part));
        }

        fn manifest(&self) -> PathBuf {
            let path = self.root.path().join("bundle/bundle.json");
            save(
                &path,
                &json!({"schema":SCHEMA,"bundle_id":"fixture-v1","case_count":1,
                "scope":{"kind":"selected_current_case_closure","full_upstream_manifest_reproduction":false,
                    "excluded":EXCLUDED,"excluded_paths":[OMITTED]},"shards":self.parts}),
            );
            path
        }

        fn target(&self) -> PathBuf {
            self.root.path().join("sources/misumi/imported")
        }

        fn mutate_descriptor(&mut self, index: usize, edit: impl FnOnce(&mut Value)) {
            let path = self.root.path().join(format!("bundle/part-{index}.json"));
            let mut part: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            edit(&mut part);
            self.parts[index] = save(&path, &part);
        }
    }

    fn cache_is_empty(root: &SkillRoot) -> bool {
        fs::read_dir(root.path().join("var/cache/knowledge"))
            .unwrap()
            .next()
            .is_none()
    }

    fn synthetic_part(member_bytes: u64) -> Part {
        let hash = sha256(b"synthetic");
        Part {
            descriptor: Bound {
                path: "descriptor.json".into(),
                sha256: hash.clone(),
                bytes: 1,
            },
            descriptor_raw: Vec::new(),
            archive: Bound {
                path: "archive.tar.xz".into(),
                sha256: hash.clone(),
                bytes: 1,
            },
            files: vec![Bound {
                path: "member.bin".into(),
                sha256: hash,
                bytes: member_bytes,
            }],
        }
    }

    #[test]
    fn bundle_parallel_import_requires_two_shards_at_threshold() {
        assert!(!parallel_import_eligible(&[]));
        assert!(!parallel_import_eligible(&[synthetic_part(
            MIN_PARALLEL_SHARD_BYTES
        )]));
        assert!(parallel_import_eligible(&[
            synthetic_part(MIN_PARALLEL_SHARD_BYTES),
            synthetic_part(MIN_PARALLEL_SHARD_BYTES),
        ]));
        assert!(!parallel_import_eligible(&[
            synthetic_part(MIN_PARALLEL_SHARD_BYTES),
            synthetic_part(MIN_PARALLEL_SHARD_BYTES - 1),
        ]));
    }

    #[test]
    fn bundle_parallel_import_bounds_workers_and_overlaps_shards() {
        let parts = [0_usize, 1, 2, 3, 4];
        let (started, starts) = mpsc::channel();
        let (finished, finishes) = mpsc::channel();
        let mut releases = Vec::new();
        let mut gates = Vec::new();
        for _ in parts {
            let (release, gate) = mpsc::channel();
            releases.push(release);
            gates.push(Mutex::new(gate));
        }
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let completed = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let runner = scope.spawn(|| {
                import_batches(&parts, |&part| {
                    let previous_completed = completed.load(Ordering::SeqCst);
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(current, Ordering::SeqCst);
                    started.send((part, previous_completed))?;
                    let released = gates[part].lock().unwrap().recv_timeout(TEST_DEADLINE);
                    active.fetch_sub(1, Ordering::SeqCst);
                    completed.fetch_add(1, Ordering::SeqCst);
                    finished.send(part)?;
                    released.context("test_worker_release_missing")
                })
            });
            for batch in parts.chunks(2) {
                let mut observed = BTreeSet::new();
                for _ in batch {
                    let (part, previous_completed) = starts.recv_timeout(TEST_DEADLINE).unwrap();
                    assert!(previous_completed >= batch[0]);
                    observed.insert(part);
                }
                assert_eq!(observed, batch.iter().copied().collect());
                // Workers have announced entry and are held by explicit gates:
                // these counts establish overlap without timing or sleeps.
                assert_eq!(active.load(Ordering::SeqCst), batch.len());
                assert!(matches!(starts.try_recv(), Err(mpsc::TryRecvError::Empty)));
                for &part in batch {
                    releases[part].send(()).unwrap();
                }
                let done = (0..batch.len())
                    .map(|_| finishes.recv_timeout(TEST_DEADLINE).unwrap())
                    .collect::<BTreeSet<_>>();
                assert_eq!(done, observed);
            }
            runner.join().unwrap().unwrap();
        });
        assert_eq!(peak.load(Ordering::SeqCst), 2);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(completed.load(Ordering::SeqCst), parts.len());

        // Real shard visitors create the same previously absent parent tree,
        // with distinct temporary Stages, and also exercise an odd last batch.
        let mut fixture = Fixture::new();
        let members = [
            "cases/cn/000210/attempts/fixed/shared/one.bin",
            "cases/cn/000210/attempts/fixed/shared/two.bin",
            "cases/cn/000210/attempts/fixed/shared/three.bin",
        ];
        fixture.shard(&[(POINTER, b"fixed"), (members[0], b"one")], false);
        fixture.shard(&[(members[1], b"two")], false);
        fixture.shard(&[(members[2], b"three")], false);
        let manifest = fixture.manifest();
        let before = fs::read(&manifest).unwrap();
        import(&fixture.root, &manifest, &fixture.target()).unwrap();
        for (member, expected) in members
            .into_iter()
            .zip([b"one".as_slice(), b"two", b"three"])
        {
            assert_eq!(fs::read(fixture.target().join(member)).unwrap(), expected);
        }
        assert_eq!(fs::read(manifest).unwrap(), before);
        assert!(cache_is_empty(&fixture.root));
    }

    #[test]
    fn bundle_parallel_import_joins_workers_before_failure_cleanup() {
        // A later worker finishes with an error or panic while its earlier peer
        // is held. Every return path must join that peer before dropping Stage.
        for (panic_later, fail_first) in [(false, false), (true, false), (false, true)] {
            let fixture = Fixture::new();
            let stage = source_pack::Stage::new(fixture.root.path(), "bundle-join-test").unwrap();
            let stage_path = stage.path.clone();
            let (started, starts) = mpsc::channel();
            let (failed, failure) = mpsc::channel();
            let (release, gate) = mpsc::channel();
            let gate = Mutex::new(gate);
            let (returned, returns) = mpsc::channel();
            let first_done = AtomicBool::new(false);
            let later_batch = AtomicBool::new(false);
            std::thread::scope(|scope| {
                let runner = scope.spawn(|| {
                    let owned_stage = stage;
                    let result = import_batches(&[0_usize, 1, 2], |&part| {
                        started.send(part)?;
                        match part {
                            0 => {
                                gate.lock().unwrap().recv_timeout(TEST_DEADLINE)?;
                                assert!(
                                    stage_path.is_dir(),
                                    "Stage dropped before its worker joined"
                                );
                                fs::write(
                                    stage_path.join("still-owned"),
                                    b"joined before cleanup",
                                )?;
                                first_done.store(true, Ordering::SeqCst);
                                if fail_first {
                                    return Err(anyhow::Error::new(std::io::Error::other(
                                        "first original error",
                                    ))
                                    .context("first original context"));
                                }
                                Ok(())
                            }
                            1 => {
                                struct Exiting<'a>(&'a mpsc::Sender<()>);
                                impl Drop for Exiting<'_> {
                                    fn drop(&mut self) {
                                        let _ = self.0.send(());
                                    }
                                }
                                let _exiting = Exiting(&failed);
                                if panic_later {
                                    panic!("fixture worker panic");
                                }
                                anyhow::bail!("later original error")
                            }
                            _ => {
                                later_batch.store(true, Ordering::SeqCst);
                                Ok(())
                            }
                        }
                    });
                    assert!(first_done.load(Ordering::SeqCst));
                    drop(owned_stage);
                    returned.send(()).unwrap();
                    result
                });
                let begun = (0..2)
                    .map(|_| starts.recv_timeout(TEST_DEADLINE).unwrap())
                    .collect::<BTreeSet<_>>();
                assert_eq!(begun, BTreeSet::from([0, 1]));
                failure.recv_timeout(TEST_DEADLINE).unwrap();
                assert!(stage_path.is_dir());
                assert!(!first_done.load(Ordering::SeqCst));
                assert!(matches!(returns.try_recv(), Err(mpsc::TryRecvError::Empty)));
                release.send(()).unwrap();
                let error = runner.join().unwrap().unwrap_err();
                if fail_first {
                    assert_eq!(error.to_string(), "first original context");
                    assert_eq!(
                        error.downcast_ref::<std::io::Error>().unwrap().to_string(),
                        "first original error"
                    );
                } else if panic_later {
                    assert_eq!(error.to_string(), "incad_bundle_worker_panicked");
                } else {
                    assert_eq!(error.to_string(), "later original error");
                }
            });
            assert!(!later_batch.load(Ordering::SeqCst));
            assert!(!stage_path.exists());
            assert!(!fixture.target().exists());
            assert!(cache_is_empty(&fixture.root));
        }

        staged_name_collision_is_exclusive();
    }

    fn staged_name_collision_is_exclusive() {
        let fixture = Fixture::new();
        let stage = source_pack::Stage::new(fixture.root.path(), "bundle-name-test").unwrap();
        let probe = stage.path.join("CaseProbe");
        fs::write(&probe, b"probe").unwrap();
        let aliases_case = stage.path.join("caseprobe").try_exists().unwrap();
        fs::remove_file(probe).unwrap();
        let targets = [
            stage.path.join("built/Case.bin"),
            stage.path.join(if aliases_case {
                "built/case.bin"
            } else {
                "built/Case.bin"
            }),
        ];
        let members = [0, 1].map(|index| {
            let path = stage.path.join(format!("source-{index}"));
            let mut file = oe_core::create_new_regular_file(&path).unwrap();
            std::io::Write::write_all(&mut file, b"same source bytes").unwrap();
            file.sync_all().unwrap();
            source_pack::VerifiedMember {
                relative_path: format!("member-{index}"),
                path,
                sha256: sha256(b"same source bytes"),
                bytes: 17,
            }
        });
        let (checked, checks) = mpsc::channel();
        let (release_a, gate_a) = mpsc::channel();
        let (release_b, gate_b) = mpsc::channel();
        let gates = [Mutex::new(gate_a), Mutex::new(gate_b)];
        let linked = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let runner = scope.spawn(|| {
                import_batches(&[0_usize, 1], |&index| {
                    assert!(!targets[index].try_exists()?);
                    checked.send(index)?;
                    gates[index].lock().unwrap().recv_timeout(TEST_DEADLINE)?;
                    retain_new_member(&members[index], &targets[index])?;
                    linked.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            });
            assert_eq!(
                (0..2)
                    .map(|_| checks.recv_timeout(TEST_DEADLINE).unwrap())
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([0, 1])
            );
            // Both absence checks have completed before either exclusive link.
            release_a.send(()).unwrap();
            release_b.send(()).unwrap();
            assert!(runner.join().unwrap().is_err());
        });
        assert_eq!(linked.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read_dir(stage.path.join("built")).unwrap().count(), 1);
        assert_eq!(fs::read(&targets[0]).unwrap(), b"same source bytes");
        for member in members {
            assert_eq!(fs::read(member.path).unwrap(), b"same source bytes");
        }
        drop(stage);
        assert!(cache_is_empty(&fixture.root));
    }

    #[test]
    fn bundle_restores_exact_selected_files_without_rewriting_upstream() {
        let mut fixture = Fixture::new();
        let upstream = b"{\"capture_file\":\"/old/private/archive/capture.json\"}";
        fixture.shard(
            &[
                (POINTER, b"fixed pointer"),
                ("cases/cn/000210/attempts/fixed/manifest.json", upstream),
            ],
            false,
        );
        fixture.shard(&[(CAD, b"ISO-10303-21; fixture\n")], false);
        let manifest = fixture.manifest();
        let original = fs::read(&manifest).unwrap();
        let verified = verify(&fixture.root, &manifest).unwrap();
        assert_eq!(verified["status"], "verified");
        assert_eq!(verified["files"], 3);
        assert_eq!(verified["source_closure_verification"], "not_run");
        assert!(!fixture.target().exists());
        let result = import(&fixture.root, &manifest, &fixture.target()).unwrap();
        assert_eq!(result["status"], "imported");
        assert_eq!(result["engineering_adoption"], "not_granted");
        assert_eq!(
            fs::read(fixture.target().join(CAD)).unwrap(),
            b"ISO-10303-21; fixture\n"
        );
        assert_eq!(
            fs::read(
                fixture
                    .target()
                    .join("cases/cn/000210/attempts/fixed/manifest.json")
            )
            .unwrap(),
            upstream
        );
        assert!(!fixture.target().join(OMITTED).exists());
        assert!(import(&fixture.root, &manifest, &fixture.target()).is_err());
        assert_eq!(fs::read(manifest).unwrap(), original);
    }

    #[test]
    fn bundle_rejects_cross_shard_duplicates_and_file_directory_conflicts_before_staging() {
        for conflicting in [POINTER.to_owned(), format!("{POINTER}/child")] {
            let mut fixture = Fixture::new();
            fixture.shard(&[(POINTER, b"original")], false);
            fixture.shard(&[(&conflicting, b"conflict")], false);
            let manifest = fixture.manifest();
            assert!(import(&fixture.root, &manifest, &fixture.target()).is_err());
            assert!(!fixture.target().exists());
            assert!(!fixture.root.path().join("var/cache/knowledge").exists());
        }
    }

    #[test]
    fn bundle_late_invalid_shard_leaves_no_published_target() {
        let mut fixture = Fixture::new();
        fixture.shard(&[(POINTER, b"first shard valid")], false);
        fixture.shard(&[(CAD, b"second shard actual bytes")], false);
        fixture.mutate_descriptor(1, |part| part["files"][0]["sha256"] = json!("0".repeat(64)));
        let manifest = fixture.manifest();
        assert!(import(&fixture.root, &manifest, &fixture.target()).is_err());
        assert!(!fixture.target().exists());
        assert_eq!(
            fs::read_dir(fixture.root.path().join("var/cache/knowledge"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn bundle_rejects_private_paths_symlink_members_and_unbound_descriptors() {
        for private in [
            OMITTED,
            "jobs/old/request.json",
            "navigation/watcher.log",
            "captures/capture.json",
            "config.json",
            "request.json",
            "current-scope.json",
            "cases/cn/000210/attempts/fixed/captures/private.json",
            "cases/cn/000210/attempts/fixed/jobs/request.json",
            "design-inputs/cn/000210/runtime/log.json",
            "navigation/assets/private_projects/project.json",
            "navigation/vendor/Browser_Capture/context.json",
        ] {
            let mut fixture = Fixture::new();
            fixture.shard(&[(POINTER, b"pointer"), (private, b"private")], false);
            assert!(verify(&fixture.root, &fixture.manifest()).is_err());
            assert!(!fixture.target().exists());
        }
        let mut fixture = Fixture::new();
        fixture.shard(&[(POINTER, b"")], true);
        assert!(import(&fixture.root, &fixture.manifest(), &fixture.target()).is_err());
        assert!(!fixture.target().exists());

        let mut fixture = Fixture::new();
        fixture.shard(&[(POINTER, b"pointer")], false);
        let manifest = fixture.manifest();
        fs::write(fixture.root.path().join("bundle/part-0.json"), b"{}").unwrap();
        assert!(verify(&fixture.root, &manifest).is_err());
    }

    #[test]
    fn bundle_declared_budgets_and_target_containment_are_enforced() {
        let mut fixture = Fixture::new();
        fixture.shard(&[(POINTER, b"pointer")], false);
        fixture.mutate_descriptor(0, |part| {
            part["files"][0]["bytes"] = json!(MAX_FILE_BYTES + 1)
        });
        assert!(verify(&fixture.root, &fixture.manifest()).is_err());
        assert!(
            import(
                &fixture.root,
                &fixture.manifest(),
                Path::new("sources/books/new")
            )
            .is_err()
        );
        assert!(!fixture.root.path().join("sources/books/new").exists());
    }
}
