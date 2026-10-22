//! The memory module is installed and versioned with the engineering system.
//! Never discover an older installation through PATH or the user's profiles.
use anyhow::{Context, Result, ensure};
use oe_core::SkillRoot;
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

fn invocation(executable: &Path, root: &Path, args: Vec<OsString>) -> Result<Command> {
    let directory = executable
        .parent()
        .context("memory_program_directory_missing")?;
    let name = if cfg!(windows) {
        "engineering-memory.exe"
    } else {
        "engineering-memory"
    };
    let program = directory.join(name);
    oe_core::no_symlinks(&program)?;
    ensure!(
        program.is_file(),
        "memory_module_not_installed: run scripts/install.sh in the selected repository"
    );
    let explicit_home = args.iter().any(|arg| {
        arg == "--home"
            || arg
                .to_str()
                .is_some_and(|value| value.starts_with("--home="))
    });
    let mut command = Command::new(program);
    if !explicit_home {
        command.arg("--home").arg(root.join("var/memory"));
    }
    command
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    Ok(command)
}

pub fn execute(root: &SkillRoot, args: Vec<OsString>) -> Result<u8> {
    let executable = std::fs::canonicalize(std::env::current_exe()?)?;
    let status = invocation(&executable, root.path(), args)?
        .status()
        .context("memory_module_execution_failed")?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sibling_never_uses_another_installed_memory() {
        let base =
            tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        assert!(invocation(&base.path().join("oev2"), base.path(), vec![]).is_err());
    }

    #[test]
    fn defaults_to_selected_root_and_preserves_explicit_store() {
        let base =
            tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let name = if cfg!(windows) {
            "engineering-memory.exe"
        } else {
            "engineering-memory"
        };
        std::fs::write(base.path().join(name), b"test executable placeholder").unwrap();
        let executable = base.path().join("oev2");
        let selected = base.path().join("selected root");
        let command = invocation(&executable, &selected, vec!["status".into()]).unwrap();
        let args: Vec<_> = command.get_args().map(OsString::from).collect();
        assert_eq!(
            args,
            vec![
                OsString::from("--home"),
                selected.join("var/memory").into_os_string(),
                "status".into()
            ]
        );
        let explicit: Vec<OsString> = vec![
            "--home".into(),
            base.path().join("shared store").into_os_string(),
            "status".into(),
        ];
        let command = invocation(&executable, &selected, explicit.clone()).unwrap();
        assert_eq!(
            command.get_args().map(OsString::from).collect::<Vec<_>>(),
            explicit
        );
    }
}
