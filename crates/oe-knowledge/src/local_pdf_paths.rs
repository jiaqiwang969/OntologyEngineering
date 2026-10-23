//! Paths emitted by local-PDF registration must also fit its readers and bindings.
//! Filename spelling is preserved; these checks do not grant filesystem access.
use anyhow::{Context, Result, ensure};
use oe_core::SkillRoot;
use std::path::{Component, Path};

const MAX_PATH_CHARS: usize = 2000;

fn validate_text(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && value.chars().count() <= MAX_PATH_CHARS
            && !value.contains('\\')
            && !value.chars().any(char::is_control),
        "local_pdf_path_invalid"
    );
    Ok(())
}

pub(crate) fn validate_root_relative(value: &str, index: bool) -> Result<()> {
    validate_text(value)?;
    ensure!(
        value
            .split('/')
            .all(|name| !name.is_empty() && name != "." && name != "..")
            && if index {
                value.starts_with("sources/.indexes/")
            } else {
                value.starts_with("sources/")
                    && value != "sources/.indexes"
                    && !value.starts_with("sources/.indexes/")
            },
        "local_pdf_path_invalid"
    );
    Ok(())
}

/// Encode only the path below the root. A legal name in the installation root
/// must not become a blanket blacklist of that installation's source paths.
pub(crate) fn root_relative_path(root: &SkillRoot, path: &Path, index: bool) -> Result<String> {
    // Registration also serializes absolute internal/output paths. Reject a
    // non-UTF-8 path explicitly before passing a Path to serde_json::json!.
    path.to_str().context("local_pdf_path_not_utf8")?;
    let relative = path
        .strip_prefix(root.path())
        .context("local_pdf_path_outside_root")?;
    let mut names = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!("local_pdf_path_invalid");
        };
        names.push(name.to_str().context("local_pdf_path_not_utf8")?);
    }
    let value = names.join("/");
    validate_root_relative(&value, index)?;
    Ok(value)
}

/// Preflight all persisted representations before extraction or publication.
/// The stored index-relative reference retains its existing `../../` contract.
pub(crate) fn preflight(root: &SkillRoot, index: &Path, original: &Path) -> Result<String> {
    root_relative_path(root, index, true)?;
    root_relative_path(root, original, false)?;
    let original_parts = original.components().collect::<Vec<_>>();
    let index_parts = index.components().collect::<Vec<_>>();
    let common = original_parts
        .iter()
        .zip(&index_parts)
        .take_while(|(a, b)| a == b)
        .count();
    let mut parts = vec![".."; index_parts.len() - common];
    for component in &original_parts[common..] {
        let Component::Normal(name) = component else {
            anyhow::bail!("local_pdf_path_invalid");
        };
        parts.push(name.to_str().context("local_pdf_path_not_utf8")?);
    }
    let stored = parts.join("/");
    validate_text(&stored)?;
    ensure!(
        crate::sources::resolve_original(root, index, &stored)? == original,
        "local_pdf_path_roundtrip_mismatch"
    );
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> SkillRoot {
        SkillRoot::new(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn local_pdf_paths_preserve_unicode_spaces_and_index_relative_roundtrip() {
        let root = root();
        let index = root.path().join("sources/.indexes/中文 索引/𠀀");
        let original = root.path().join("sources/books/工程 𠀀 原件.pdf");
        assert_eq!(
            root_relative_path(&root, &original, false).unwrap(),
            "sources/books/工程 𠀀 原件.pdf"
        );
        assert_eq!(
            root_relative_path(&root, &index, true).unwrap(),
            "sources/.indexes/中文 索引/𠀀"
        );
        assert_eq!(
            preflight(&root, &index, &original).unwrap(),
            "../../../books/工程 𠀀 原件.pdf"
        );
        #[cfg(unix)]
        assert_eq!(
            preflight(
                &root,
                &root.path().join("sources/.indexes/edition:1"),
                &root.path().join("sources/books/book:1.pdf"),
            )
            .unwrap(),
            "../../books/book:1.pdf"
        );
    }

    #[test]
    #[cfg(unix)]
    fn local_pdf_paths_do_not_blacklist_installation_root_names() {
        let owner = root();
        let stage = crate::source_pack::Stage::new(owner.path(), "local-pdf-path-root").unwrap();
        let installation = stage.path.join("安装 root\\name\n:𠀀");
        std::fs::create_dir(&installation).unwrap();
        std::fs::write(installation.join("SKILL.md"), "owned path fixture").unwrap();
        let root = SkillRoot::new(&installation).unwrap();
        let original = root.path().join("sources/books/工程 原件.pdf");
        let index = root.path().join("sources/.indexes/中文 索引");
        assert_eq!(
            root_relative_path(&root, &original, false).unwrap(),
            "sources/books/工程 原件.pdf"
        );
        assert_eq!(
            preflight(&root, &index, &original).unwrap(),
            "../../books/工程 原件.pdf"
        );
        // Stage owns and cleans the private fixture; no original is created.
    }

    #[test]
    fn local_pdf_paths_reject_unreadable_components_and_non_utf8() {
        let root = root();
        for name in ["line\nbreak", "tab\tname", "control\u{0085}"] {
            let original = root.path().join("sources/books").join(name);
            let index = root.path().join("sources/.indexes").join(name);
            assert!(root_relative_path(&root, &original, false).is_err());
            assert!(root_relative_path(&root, &index, true).is_err());
        }
        for path in [
            "sources/../var/original.pdf",
            "sources/.indexes/index/original.pdf",
            "var/original.pdf",
        ] {
            assert!(root_relative_path(&root, &root.path().join(path), false).is_err());
        }
        assert!(root_relative_path(&root, &root.path().join("sources/.indexes"), true).is_err());
        #[cfg(unix)]
        {
            use std::{ffi::OsString, os::unix::ffi::OsStringExt};
            // Backslash is a filename character on Unix; on Windows a native
            // path separator is encoded as '/' rather than misread as a name.
            for (base, is_index) in [("sources/books", false), ("sources/.indexes", true)] {
                assert!(
                    root_relative_path(
                        &root,
                        &root.path().join(base).join("back\\slash"),
                        is_index
                    )
                    .is_err()
                );
            }
            let name = OsString::from_vec(vec![b'p', 0xff]);
            for (base, is_index) in [("sources/books", false), ("sources/.indexes", true)] {
                let error =
                    root_relative_path(&root, &root.path().join(base).join(&name), is_index)
                        .unwrap_err();
                assert_eq!(error.to_string(), "local_pdf_path_not_utf8");
            }
        }
    }

    #[test]
    fn local_pdf_path_limits_count_characters_and_check_final_relative_form() {
        // Scalar-character limits preserve non-ASCII names instead of treating
        // multi-byte UTF-8 as multiple filename characters.
        assert!(validate_text(&"书".repeat(MAX_PATH_CHARS)).is_ok());
        assert!(validate_text(&"书".repeat(MAX_PATH_CHARS + 1)).is_err());
        let root = root();
        let index = (0..300).fold(root.path().join("sources/.indexes"), |path, _| {
            path.join("i")
        });
        let original = (0..7)
            .fold(root.path().join("sources/books"), |path, _| {
                path.join("b".repeat(200))
            })
            .join("original.pdf");
        assert!(root_relative_path(&root, &index, true).is_ok());
        assert!(root_relative_path(&root, &original, false).is_ok());
        assert_eq!(
            preflight(&root, &index, &original).unwrap_err().to_string(),
            "local_pdf_path_invalid"
        );
    }
}
