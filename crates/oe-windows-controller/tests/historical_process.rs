#![cfg(windows)]
use oe_windows_controller::windows::*;
use std::process::{Command, Stdio};

#[test]
fn owned_short_process() {
    if std::env::var_os("OE_OWNED_PROCESS_IDENTITY_CHILD").is_some() {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

#[test]
fn fresh_historical_observation_binds_pid_and_creation_without_killing() {
    let own = ProcessWatch::current().unwrap();
    assert_eq!(
        ProcessWatch::observe_historical(own.identity()).unwrap(),
        HistoricalProcessObservation::LiveSameIdentity
    );
    let mut different = own.identity();
    different.creation_time_100ns += 1;
    assert_eq!(
        ProcessWatch::observe_historical(different).unwrap(),
        HistoricalProcessObservation::DifferentCreation(own.identity())
    );
    assert!(
        ProcessWatch::observe_historical(ProcessIdentity {
            pid: 0,
            creation_time_100ns: 1
        })
        .is_err()
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "owned_short_process", "--nocapture"])
        .env("OE_OWNED_PROCESS_IDENTITY_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let retained = ProcessWatch::from_child(&child).unwrap();
    assert_eq!(
        ProcessWatch::observe_historical(retained.identity()).unwrap(),
        HistoricalProcessObservation::LiveSameIdentity
    );
    assert_eq!(child.wait().unwrap().code(), Some(0));
    assert_eq!(
        ProcessWatch::observe_historical(retained.identity()).unwrap(),
        HistoricalProcessObservation::ExitedSameIdentity { exit_code: 0 }
    );
}
