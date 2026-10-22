//! Explicit synthetic-process tests only. Never loads or starts NX.
#[cfg(windows)]
mod probe {
    use oe_windows_controller::windows::*;
    use std::{
        ffi::OsStr,
        fs, io,
        path::{Path, PathBuf},
        process::{Command, ExitCode, Stdio},
        thread,
        time::{Duration, Instant},
    };
    fn require(value: bool, name: &str) -> io::Result<()> {
        if value {
            Ok(())
        } else {
            Err(io::Error::other(name.to_owned()))
        }
    }
    fn string(s: &str) -> String {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    fn check(checks: &mut Vec<String>, name: &str, value: bool) -> io::Result<()> {
        require(value, name)?;
        checks.push(name.into());
        println!("{{\"check\":{},\"passed\":true}}", string(name));
        Ok(())
    }
    fn wait_file(path: &Path) -> io::Result<()> {
        let start = Instant::now();
        while !path.is_file() {
            if start.elapsed() > Duration::from_secs(10) {
                return Err(io::Error::other("owned_child_ready_timeout"));
            }
            thread::sleep(Duration::from_millis(20))
        }
        Ok(())
    }
    fn owned_child(mode: &str, path: &Path, args: &[&str]) -> io::Result<std::process::Child> {
        Command::new(std::env::current_exe()?)
            .arg(mode)
            .arg(path)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
    fn case(base: &Path, name: &str) -> io::Result<PathBuf> {
        let p = base.join(name);
        fs::create_dir(&p)?;
        Ok(p)
    }
    pub fn execute() -> io::Result<ExitCode> {
        let args: Vec<_> = std::env::args_os().collect();
        require(args.len() >= 3, "explicit_mode_and_owned_root_required")?;
        let mode = args[1]
            .to_str()
            .ok_or_else(|| io::Error::other("mode_encoding"))?;
        let path = Path::new(&args[2]);
        match mode {
            "wmi-detach" => {
                require(!path.exists(), "new_WMI_synthetic_root_required")?;
                fs::create_dir(path)?;
                let watch = oe_windows_controller::wmi::spawn_supervisor(
                    &std::env::current_exe()?,
                    &["detached-child".into(), path.as_os_str().to_owned()],
                    path,
                )?;
                wait_file(&path.join("started.json"))?;
                println!(
                    "{{\"pid\":{},\"creation_time_100ns\":{},\"in_job\":{},\"native_com\":true}}",
                    watch.identity().pid,
                    watch.identity().creation_time_100ns,
                    watch.in_job()?
                );
                Ok(ExitCode::SUCCESS)
            }
            "detached-child" => {
                require(path.is_dir(), "owned_detached_root_required")?;
                let watch = ProcessWatch::current()?;
                let in_job = watch.in_job()?;
                fs::write(
                    path.join("started.json"),
                    format!(
                        "{{\"pid\":{},\"creation_time_100ns\":{},\"in_job\":{}}}",
                        watch.identity().pid,
                        watch.identity().creation_time_100ns,
                        in_job
                    ),
                )?;
                thread::sleep(Duration::from_secs(15));
                fs::write(
                    path.join("finished.json"),
                    format!(
                        "{{\"pid\":{},\"creation_time_100ns\":{},\"reached_after_parent_disconnect\":true}}",
                        watch.identity().pid,
                        watch.identity().creation_time_100ns
                    ),
                )?;
                Ok(ExitCode::SUCCESS)
            }
            "sleep" => {
                thread::sleep(Duration::from_millis(750));
                Ok(ExitCode::SUCCESS)
            }
            "try-lock" => {
                let root = StableRoot::open(path, None)?;
                match root.lock() {
                    Ok(_) => {
                        println!("{{\"lock\":\"acquired\"}}");
                        Ok(ExitCode::SUCCESS)
                    }
                    Err(e) if e.raw_os_error() == Some(32) => {
                        println!("{{\"lock\":\"sharing_violation\",\"windows_error\":32}}");
                        Ok(ExitCode::from(32))
                    }
                    Err(e) => Err(e),
                }
            }
            "root-check" => {
                let root = StableRoot::open(path, None)?;
                println!(
                    "{{\"root\":\"accepted\",\"host_id\":{},\"volume_id\":{},\"directory_id\":{}}}",
                    string(&root.identity().host_id),
                    string(&root.identity().volume_id),
                    string(&root.identity().directory_id)
                );
                Ok(ExitCode::SUCCESS)
            }
            "hold-lock" => {
                let root = StableRoot::open(path, None)?;
                let _lock = root.lock()?;
                fs::write(path.join("ready.signal"), b"synthetic lock owner ready")?;
                let start = Instant::now();
                while !path.join("release.signal").is_file() {
                    if start.elapsed() > Duration::from_secs(20) {
                        return Err(io::Error::other("owned_lock_release_timeout"));
                    }
                    thread::sleep(Duration::from_millis(20))
                }
                Ok(ExitCode::SUCCESS)
            }
            "interrupt-write" => {
                require(args.len() == 4, "checkpoint_required")?;
                let chosen = args[3]
                    .to_str()
                    .ok_or_else(|| io::Error::other("checkpoint_encoding"))?;
                let root = StableRoot::open(path, None)?;
                let lock = root.lock()?;
                let dir = root.directory(OsStr::new("job"))?;
                write_state_with_checkpoint(
                    &dir,
                    &lock,
                    None,
                    b"{\"schema\":\"synthetic-platform-state\",\"state\":\"STARTING\"}",
                    |point| {
                        let hit = matches!(
                            (chosen, point),
                            ("pending", WriteCheckpoint::PendingSynced)
                                | ("temporary", WriteCheckpoint::TemporarySynced)
                                | ("replaced", WriteCheckpoint::ReplacedAndReadback)
                        );
                        if hit {
                            std::process::exit(31)
                        }
                        Ok(())
                    },
                )?;
                Err(io::Error::other("interrupt_checkpoint_not_reached"))
            }
            "selftest" => {
                selftest(path)?;
                Ok(ExitCode::SUCCESS)
            }
            _ => Err(io::Error::other("unknown_synthetic_probe_mode")),
        }
    }
    fn powershell(script: &str) -> io::Result<std::process::Child> {
        let exe = PathBuf::from(
            std::env::var_os("SystemRoot").ok_or_else(|| io::Error::other("systemroot_missing"))?,
        )
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        Command::new(exe)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    }
    fn ps_path(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "''"))
    }
    fn selftest(base: &Path) -> io::Result<()> {
        require(!base.exists(), "synthetic_test_root_must_be_new")?;
        fs::create_dir(base)?;
        let mut checks = Vec::new();
        let root_path = case(base, "目录-é-稳定根")?;
        let root = StableRoot::open(&root_path, None)?;
        let alias = root_path
            .parent()
            .unwrap()
            .join(".")
            .join(root_path.file_name().unwrap());
        let again = StableRoot::open(&alias, Some(root.identity()))?;
        check(
            &mut checks,
            "unicode_and_dot_alias_bind_same_directory",
            again.identity() == root.identity(),
        )?;
        drop(again);
        let mut wrong = root.identity().clone();
        wrong.directory_id.push('0');
        check(
            &mut checks,
            "changed_root_identity_rejected",
            StableRoot::open(&root_path, Some(&wrong)).is_err(),
        )?;
        {
            let lock = root.lock()?;
            check(
                &mut checks,
                "same_process_zero_share_conflict",
                root.lock().is_err_and(|e| e.raw_os_error() == Some(32)),
            )?;
            let mut child = owned_child("try-lock", &root_path, &[])?;
            check(
                &mut checks,
                "different_process_zero_share_conflict",
                child.wait()?.code() == Some(32),
            )?;
            lock.verify()?;
        }
        {
            let lock = root.lock()?;
            let mut child = owned_child("sleep", &root_path, &[])?;
            let watch = ProcessWatch::from_child(&child)?;
            let identity = watch.identity();
            check(
                &mut checks,
                "held_handle_observes_running",
                watch.poll()? == ProcessObservation::Running,
            )?;
            let second = ProcessWatch::open(identity)?;
            check(
                &mut checks,
                "pid_creationtime_reopen_matches",
                second.identity() == identity,
            )?;
            let forged = ProcessIdentity {
                creation_time_100ns: identity.creation_time_100ns + 1,
                ..identity
            };
            check(
                &mut checks,
                "same_pid_wrong_creationtime_rejected",
                ProcessWatch::open(forged).is_err(),
            )?;
            drop(lock);
            let new_lock = root.lock()?;
            check(
                &mut checks,
                "lock_handle_not_inherited_by_child",
                watch.poll()? == ProcessObservation::Running,
            )?;
            drop(new_lock);
            require(child.wait()?.success(), "owned_sleep_failed")?;
            check(
                &mut checks,
                "held_handle_confirms_actual_exit",
                watch.poll()? == ProcessObservation::Exited { exit_code: 0 },
            )?;
            println!(
                "{{\"process_pid\":{},\"creation_time_100ns\":{},\"exit_code\":0}}",
                identity.pid, identity.creation_time_100ns
            );
        }
        let pinned = case(&root_path, "固定目录")?;
        let pin = root.directory(OsStr::new("固定目录"))?;
        check(
            &mut checks,
            "pinned_directory_rename_denied",
            fs::rename(&pinned, root_path.join("renamed")).is_err(),
        )?;
        drop(pin);
        fs::rename(&pinned, root_path.join("renamed"))?;
        check(
            &mut checks,
            "released_directory_rename_allowed",
            root_path.join("renamed").is_dir(),
        )?;
        let state_root = case(base, "state-publication")?;
        fs::create_dir(state_root.join("job"))?;
        let state_root = StableRoot::open(&state_root, None)?;
        let lock = state_root.lock()?;
        let dir = state_root.directory(OsStr::new("job"))?;
        let first = b"{\"synthetic\":1}";
        let second = b"{\"synthetic\":2}";
        write_state_atomic(&dir, &lock, None, first)?;
        check(
            &mut checks,
            "atomic_first_write_readback",
            dir.read_record(OsStr::new(STATE_NAME))?.as_deref() == Some(first),
        )?;
        write_state_atomic(&dir, &lock, Some(first), second)?;
        check(
            &mut checks,
            "atomic_replace_readback",
            dir.read_record(OsStr::new(STATE_NAME))?.as_deref() == Some(second),
        )?;
        check(
            &mut checks,
            "committed_update_has_no_pending",
            dir.read_record(OsStr::new(PENDING_NAME))?.is_none(),
        )?;
        check(
            &mut checks,
            "stale_compare_rejected_without_update",
            write_state_atomic(&dir, &lock, Some(first), first).is_err()
                && dir.read_record(OsStr::new(STATE_NAME))?.as_deref() == Some(second)
                && dir.read_record(OsStr::new(PENDING_NAME))?.is_none(),
        )?;
        let third = b"{\"synthetic\":3}";
        let snap = dir.read_record_with_checkpoint(OsStr::new(STATE_NAME), || {
            write_state_atomic(&dir, &lock, Some(second), third)
        })?;
        check(
            &mut checks,
            "snapshot_reader_does_not_block_atomic_replace",
            snap.as_deref() == Some(second)
                && dir.read_record(OsStr::new(STATE_NAME))?.as_deref() == Some(third),
        )?;
        drop(dir);
        drop(lock);
        drop(state_root);
        for point in ["pending", "temporary", "replaced"] {
            let interrupted = case(base, &format!("interrupt-{point}"))?;
            fs::create_dir(interrupted.join("job"))?;
            let mut child = owned_child("interrupt-write", &interrupted, &[point])?;
            let watch = ProcessWatch::from_child(&child)?;
            require(
                child.wait()?.code() == Some(31),
                "synthetic_interrupt_failed",
            )?;
            check(
                &mut checks,
                &format!("{point}_interrupt_exit_confirmed"),
                watch.poll()? == ProcessObservation::Exited { exit_code: 31 },
            )?;
            let r = StableRoot::open(&interrupted, None)?;
            let guard = r.lock()?;
            let facts = scan_writer_records(&guard)?;
            check(
                &mut checks,
                &format!("{point}_interrupt_retains_pending"),
                facts.len() == 1 && facts[0].pending.is_some() && facts[0].error.is_none(),
            )?;
            if point == "pending" {
                check(
                    &mut checks,
                    "before_replacement_has_no_state",
                    facts[0].state.is_none(),
                )?
            }
            if point == "replaced" {
                check(
                    &mut checks,
                    "after_replacement_keeps_state_and_pending",
                    facts[0].state.is_some(),
                )?
            }
        }
        let held_root = case(base, "child-lock-owner")?;
        let mut child = owned_child("hold-lock", &held_root, &[])?;
        wait_file(&held_root.join("ready.signal"))?;
        let r = StableRoot::open(&held_root, None)?;
        check(
            &mut checks,
            "foreign_owned_process_lock_blocks_parent",
            r.lock().is_err_and(|e| e.raw_os_error() == Some(32)),
        )?;
        fs::write(held_root.join("release.signal"), b"release")?;
        require(child.wait()?.success(), "owned_lock_child_failed")?;
        let guard = r.lock()?;
        guard.verify()?;
        check(&mut checks, "ended_process_releases_only_kernel_lock", true)?;
        drop(guard);
        let interop = case(base, "dotnet-lock-interop")?;
        let interop_root = StableRoot::open(&interop, None)?;
        let native_lock = interop_root.lock()?;
        let script = format!(
            "try{{$f=[IO.File]::Open({},'OpenOrCreate','ReadWrite','None');$f.Dispose();exit 1}}catch{{if(($_.Exception.InnerException.HResult -band 65535) -eq 32){{exit 0}};exit 2}}",
            ps_path(&interop.join(LOCK_NAME))
        );
        check(
            &mut checks,
            "dotnet_FileShare_None_conflicts_with_rust_lock",
            powershell(&script)?.wait()?.success(),
        )?;
        drop(native_lock);
        let ready = interop.join("ready");
        let release = interop.join("release");
        let script = format!(
            "$f=[IO.File]::Open({},'OpenOrCreate','ReadWrite','None');try{{[IO.File]::WriteAllText({},'ready');$s=[Diagnostics.Stopwatch]::StartNew();while(!(Test-Path -LiteralPath {})){{if($s.Elapsed.TotalSeconds -gt 20){{exit 2}};Start-Sleep -Milliseconds 20}}}}finally{{$f.Dispose()}};exit 0",
            ps_path(&interop.join(LOCK_NAME)),
            ps_path(&ready),
            ps_path(&release)
        );
        let mut dotnet = powershell(&script)?;
        wait_file(&ready)?;
        check(
            &mut checks,
            "rust_lock_conflicts_with_dotnet_FileShare_None",
            interop_root
                .lock()
                .is_err_and(|e| e.raw_os_error() == Some(32)),
        )?;
        fs::write(&release, b"release")?;
        check(
            &mut checks,
            "dotnet_owner_ends_normally",
            dotnet.wait()?.success(),
        )?;
        let file_root_path = case(base, "pinned-files")?;
        fs::create_dir(file_root_path.join("job"))?;
        let file_root = StableRoot::open(&file_root_path, None)?;
        let job = file_root.directory(OsStr::new("job"))?;
        let out = job.create_directory_new(OsStr::new("out"))?;
        out.create_file_new(OsStr::new("构件-é.bin"), b"bounded data", 64)?;
        check(
            &mut checks,
            "output_create_new_never_overwrites",
            out.create_file_new(OsStr::new("构件-é.bin"), b"replace", 64)
                .is_err(),
        )?;
        let mut held = out.pin_file(OsStr::new("构件-é.bin"), 64)?.unwrap();
        check(
            &mut checks,
            "pinned_file_bytes_and_identity",
            held.read_all(64)? == b"bounded data" && !held.identity().file_id.is_empty(),
        )?;
        check(
            &mut checks,
            "pinned_file_rejects_write",
            fs::write(held.path(), b"changed").is_err(),
        )?;
        check(
            &mut checks,
            "pinned_file_rejects_delete",
            fs::remove_file(held.path()).is_err(),
        )?;
        check(
            &mut checks,
            "pinned_file_explicit_bound",
            out.pin_file(OsStr::new("构件-é.bin"), 2).is_err(),
        )?;
        check(
            &mut checks,
            "nested_directory_rename_denied",
            fs::rename(job.path(), file_root.path().join("changed")).is_err(),
        )?;
        drop(held);
        fs::hard_link(
            out.path().join("构件-é.bin"),
            out.path().join("hardlink.bin"),
        )?;
        check(
            &mut checks,
            "multi_link_file_rejected",
            out.pin_file(OsStr::new("构件-é.bin"), 64).is_err(),
        )?;
        let exact = vec![b'x'; RECORD_LIMIT as usize];
        out.create_file_new(OsStr::new("max.json"), &exact, RECORD_LIMIT)?;
        check(
            &mut checks,
            "record_accepts_shared_protocol_4MiB_limit",
            out.read_record(OsStr::new("max.json"))?.unwrap().len() == RECORD_LIMIT as usize,
        )?;
        fs::write(
            out.path().join("oversize.json"),
            vec![b'x'; RECORD_LIMIT as usize + 1],
        )?;
        check(
            &mut checks,
            "record_rejects_beyond_shared_4MiB_limit",
            out.read_record(OsStr::new("oversize.json")).is_err(),
        )?;
        let current = ProcessWatch::current()?;
        check(
            &mut checks,
            "current_process_identity_observed",
            current.identity().pid == std::process::id()
                && current.poll()? == ProcessObservation::Running,
        )?;
        let junction = file_root_path.join("junction");
        let script = format!(
            "$ErrorActionPreference='Stop';New-Item -ItemType Junction -Path {} -Target {} | Out-Null",
            ps_path(&junction),
            ps_path(&file_root_path.join("job"))
        );
        let status = powershell(&script)?.wait()?;
        require(status.success(), "owned_junction_creation_failed")?;
        check(
            &mut checks,
            "root_rejects_reparse_junction",
            StableRoot::open(&junction, None).is_err(),
        )?;
        let scan_lock = file_root.lock()?;
        let records = scan_writer_records(&scan_lock)?;
        check(
            &mut checks,
            "scanner_reports_reparse_instead_of_following",
            records
                .iter()
                .any(|r| r.name == OsStr::new("junction") && r.error.is_some()),
        )?;
        let hidden = file_root_path.join("hidden-job");
        fs::create_dir(&hidden)?;
        let script = format!(
            "$p={};[IO.File]::SetAttributes($p,[IO.FileAttributes]::Directory -bor [IO.FileAttributes]::Hidden)",
            ps_path(&hidden)
        );
        require(
            powershell(&script)?.wait()?.success(),
            "set_hidden_fixture_failed",
        )?;
        let records = scan_writer_records(&scan_lock)?;
        check(
            &mut checks,
            "scanner_does_not_skip_hidden_directory",
            records.iter().any(|r| {
                r.name == OsStr::new("hidden-job") && r.state.is_none() && r.error.is_none()
            }),
        )?;
        println!(
            "{{\"status\":\"platform_primitives_passed\",\"checks\":{},\"nx_started\":false,\"protocol_adjudication\":\"separate_shared_library_required\",\"power_loss_durability\":\"not_tested\",\"actual_pid_reuse\":\"not_forced_wrong_creationtime_rejected\"}}",
            checks.len()
        );
        Ok(())
    }
    pub fn main() -> ExitCode {
        match execute() {
            Ok(code) => code,
            Err(e) => {
                println!(
                    "{{\"status\":\"failed\",\"error\":{},\"windows_error\":{}}}",
                    string(&e.to_string()),
                    e.raw_os_error()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "null".into())
                );
                ExitCode::from(2)
            }
        }
    }
}
#[cfg(windows)]
fn main() -> std::process::ExitCode {
    probe::main()
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows-only platform primitive probe; no Windows behavior tested here.");
    std::process::exit(2)
}
