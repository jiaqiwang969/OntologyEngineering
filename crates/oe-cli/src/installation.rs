//! Private installation identity for a source checkout. No global configuration.
use anyhow::{Context, Result, ensure};
use oe_contracts::{UnifiedInstallation, UnifiedProgram};
use oe_core::SkillRoot;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

const PROGRAMS: &[&str] = &[
    "oev2",
    "oev2-design-bootstrap",
    "oe-code-runner",
    "oe-code-evaluator",
    "oe-evolution-driver",
    "oe-runtime-linker2",
    "oe-runtime-rustc2",
    "engineering-memory",
];

fn program(directory: &Path, name: &str) -> Result<UnifiedProgram> {
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let path = directory.join(filename);
    oe_core::no_symlinks(&path)?;
    let mut file = std::fs::File::open(&path)
        .with_context(|| format!("installation_program_missing: {name}"))?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() > 0 && before.len() <= 1024 * 1024 * 1024,
        "installation_program_invalid"
    );
    let mut digest = Sha256::new();
    let bytes = std::io::copy(
        &mut Read::by_ref(&mut file).take(before.len() + 1),
        &mut digest,
    )?;
    let after = std::fs::metadata(&path)?;
    ensure!(
        bytes == before.len()
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "installation_program_changed"
    );
    Ok(UnifiedProgram {
        name: name.into(),
        path: path.to_str().context("installation_path_not_utf8")?.into(),
        sha256: format!("{:x}", digest.finalize()),
        bytes,
    })
}

fn capture(root: &Path, directory: &Path) -> Result<UnifiedInstallation> {
    Ok(UnifiedInstallation {
        schema: "oev2.installation/1".into(),
        root: root.to_str().context("installation_path_not_utf8")?.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        programs: PROGRAMS
            .iter()
            .map(|name| program(directory, name))
            .collect::<Result<_>>()?,
    })
}

fn register(root: &SkillRoot, directory: &Path) -> Result<serde_json::Value> {
    let value = serde_json::to_value(capture(root.path(), directory)?)?;
    let path = root.path().join("var/install/INSTALLATION.json");
    if path.exists() {
        let previous = oe_core::read_regular_bytes(&path, 1024 * 1024)?;
        let previous: UnifiedInstallation = serde_json::from_slice(&previous)?;
        ensure!(
            serde_json::to_value(previous)? == value,
            "installation_identity_changed: preserve the previous installation and run scripts/install.sh"
        );
    } else {
        oe_core::write_new(&path, &value)?;
    }
    Ok(value)
}

pub fn execute(root: &SkillRoot) -> Result<u8> {
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    let directory = executable
        .parent()
        .context("installation_program_directory_missing")?;
    // A source installation must belong to the selected checkout. A release
    // bundle has its own release manifest and installer; do not mix identities.
    ensure!(
        directory == root.path().join("var/install/bin"),
        "installation_run_installed_entry: run scripts/install.sh first"
    );
    let _binding = crate::v2::require_execution(root, "unified_installation")?;
    println!(
        "{}",
        serde_json::to_string_pretty(&register(root, directory)?)?
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unified_installation_missing_or_changed_program() {
        let temporary = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let base = tempfile::tempdir_in(temporary).unwrap();
        std::fs::write(
            base.path().join("SKILL.md"),
            b"---\nname: ontology-engineering-v2\n---\n",
        )
        .unwrap();
        let root = SkillRoot::new(base.path()).unwrap();
        let bin = root.path().join("var/install/bin");
        std::fs::create_dir_all(&bin).unwrap();
        assert!(register(&root, &bin).is_err());
        assert!(!root.path().join("var/install/INSTALLATION.json").exists());
        for name in PROGRAMS {
            let filename = if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_string()
            };
            std::fs::write(bin.join(filename), name.as_bytes()).unwrap();
        }
        let recorded = register(&root, &bin).unwrap();
        assert_eq!(recorded["programs"].as_array().unwrap().len(), 8);
        assert_eq!(recorded, register(&root, &bin).unwrap());
        let memory = if cfg!(windows) {
            "engineering-memory.exe"
        } else {
            "engineering-memory"
        };
        std::fs::write(bin.join(memory), b"a different build").unwrap();
        assert!(register(&root, &bin).is_err());
        let stored: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.path().join("var/install/INSTALLATION.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(stored, recorded);
    }
}
