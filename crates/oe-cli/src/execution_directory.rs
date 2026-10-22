//! Compatibility entry; all production directory allocation uses oe-core.
use anyhow::Result;
use oe_core::SkillRoot;
use std::path::PathBuf;

pub(super) fn create(root: &SkillRoot, role: &str) -> Result<PathBuf> {
    root.unique_private_dir(
        &PathBuf::from(format!("var/maintenance/executions/{role}")),
        "",
    )
}
#[cfg(test)]
const COLLISION_ATTEMPTS: u32 = oe_core::PRIVATE_DIRECTORY_ATTEMPTS;
#[cfg(test)]
fn allocate(root: &SkillRoot, role: &str, pid: u32, nanos: u128) -> Result<PathBuf> {
    root.reserve_private_directory(
        &PathBuf::from(format!("var/maintenance/executions/{role}")),
        &format!("{pid}-{nanos}"),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn isolated_root() -> (tempfile::TempDir, SkillRoot) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();
        fs::write(path.join("SKILL.md"), "isolated allocation counterexample").unwrap();
        let root = SkillRoot::new(path).unwrap();
        (directory, root)
    }

    #[test]
    fn execution_directory_same_pid_history_and_collisions_create_new_records() {
        let (_directory, root) = isolated_root();
        let pid = std::process::id();
        let role = "formal_semantic_execution";
        let old = root
            .new_private_dir(&PathBuf::from(format!(
                "var/maintenance/executions/{role}/{pid}"
            )))
            .unwrap();
        fs::write(old.join("binding.json"), b"original pid-only history\n").unwrap();
        // A fixed timestamp also covers coarse clocks and simultaneous callers.
        let first = allocate(&root, role, pid, 123).unwrap();
        fs::write(first.join("binding.json"), b"first receipt\n").unwrap();
        let second = allocate(&root, role, pid, 123).unwrap();
        assert_eq!(first.file_name().unwrap(), format!("{pid}-123-0").as_str());
        assert_eq!(second.file_name().unwrap(), format!("{pid}-123-1").as_str());
        assert_eq!(
            fs::read(old.join("binding.json")).unwrap(),
            b"original pid-only history\n"
        );
        assert_eq!(
            fs::read(first.join("binding.json")).unwrap(),
            b"first receipt\n"
        );
        assert!(second.read_dir().unwrap().next().is_none());
        let third = create(&root, role).unwrap();
        assert_ne!(third, old);
        assert_ne!(third, first);
        assert_ne!(third, second);
    }

    #[test]
    fn execution_directory_collision_budget_never_overwrites_history() {
        let (_directory, root) = isolated_root();
        for attempt in 0..COLLISION_ATTEMPTS {
            let path = root
                .new_private_dir(&PathBuf::from(format!(
                    "var/maintenance/executions/role/7-123-{attempt}"
                )))
                .unwrap();
            fs::write(path.join("binding.json"), attempt.to_le_bytes()).unwrap();
        }
        let error = allocate(&root, "role", 7, 123).unwrap_err();
        assert_eq!(
            error.to_string(),
            "execution_directory_collision_budget_exceeded"
        );
        let parent = root.path().join("var/maintenance/executions/role");
        assert_eq!(
            parent.read_dir().unwrap().count(),
            COLLISION_ATTEMPTS as usize
        );
        for attempt in 0..COLLISION_ATTEMPTS {
            assert_eq!(
                fs::read(parent.join(format!("7-123-{attempt}/binding.json"))).unwrap(),
                attempt.to_le_bytes()
            );
        }
        assert!(!parent.join("7-123-32").exists());
    }

    #[test]
    fn execution_directory_path_and_link_guards_do_not_become_collision_retries() {
        let (_directory, root) = isolated_root();
        assert!(allocate(&root, "../escape", 7, 123).is_err());
        assert!(!root.path().join("var/maintenance/escape").exists());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            let parent = root.path().join("var/maintenance/executions");
            fs::create_dir_all(&parent).unwrap();
            std::os::unix::fs::symlink(outside.path(), parent.join("linked")).unwrap();
            assert!(allocate(&root, "linked", 7, 123).is_err());
            assert!(outside.path().read_dir().unwrap().next().is_none());
        }
    }
}
