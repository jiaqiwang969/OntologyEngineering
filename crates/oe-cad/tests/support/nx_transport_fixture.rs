//! Synthetic local I/O fixture. Never loads NX, invokes SSH or serves production.
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::Duration,
};
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if std::path::Path::new(&args[0])
        .file_name()
        .is_some_and(|n| n == "ssh")
    {
        if let Some(mode) = args.iter().find(|a| a.starts_with("fixture-run-")) {
            let (request, payload) = oe_nx_protocol::read_request(&mut std::io::stdin()).unwrap();
            let oe_nx_protocol::Request::Run { job_id, .. } = request else {
                panic!("expected synthetic Run")
            };
            assert!(payload.is_empty());
            let mut counter = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("var/request-count")
                .unwrap();
            counter.write_all(b"1").unwrap();
            counter.sync_all().unwrap();
            match mode.as_str() {
                "fixture-run-exit" => {
                    print!("partial synthetic response");
                    std::process::exit(17);
                }
                "fixture-run-malformed" => {
                    print!("invalid synthetic frame");
                    return;
                }
                "fixture-run-persist" => {
                    std::fs::create_dir("var/run/run-observation.json").unwrap();
                }
                "fixture-run-error" | "fixture-run-binding" | "fixture-run-unknown" => {}
                _ => panic!("unknown synthetic Run mode"),
            }
            let record = if mode == "fixture-run-error" {
                serde_json::json!({"schema":"cad-agent.nx-error/v2","error":"private synthetic peer detail","automatic_retry":false})
            } else {
                serde_json::json!({
                    "schema":"cad-agent.nx-status-unknown/v2",
                    "job_id": if mode == "fixture-run-binding" { "different-job" } else { job_id.as_str() },
                    "root":{"host_id":"fixture-host","volume_id":"fixture-volume","directory_id":"fixture-root"},
                    "state_file":null,"blocks_writes":true,"reason":"synthetic state unavailable","automatic_retry":false
                })
            };
            oe_nx_protocol::write_frame(
                &mut std::io::stdout(),
                &oe_nx_protocol::Frame {
                    record,
                    payload: vec![],
                },
            )
            .unwrap();
            return;
        }
        assert!(args.iter().any(|a| a == "fixture-error-host"));
        let (request, payload) = oe_nx_protocol::read_request(&mut std::io::stdin()).unwrap();
        assert!(matches!(request, oe_nx_protocol::Request::Status { .. }));
        assert!(payload.is_empty());
        let response = oe_nx_protocol::Frame {
            record: serde_json::json!({"schema":"cad-agent.nx-error/v2","error":"synthetic remote observation unavailable","automatic_retry":false}),
            payload: vec![],
        };
        oe_nx_protocol::write_frame(&mut std::io::stdout(), &response).unwrap();
        return;
    }
    match args[1].as_str() {
        "framed" | "framed-trailing" => {
            let (request, payload) = oe_nx_protocol::read_request(&mut std::io::stdin()).unwrap();
            let frame = oe_nx_protocol::Frame {
                record: serde_json::json!({"schema":"fixture.response/v1","request":request,"payload":oe_nx_protocol::BlobIdentity::of(&payload)}),
                payload: vec![],
            };
            oe_nx_protocol::write_frame(&mut std::io::stdout(), &frame).unwrap();
            if args[1] == "framed-trailing" {
                print!("unexpected text");
            }
        }
        "duplex" => {
            let block = vec![b'e'; 256 * 1024];
            std::io::stderr().write_all(&block).unwrap();
            let mut bytes = Vec::new();
            std::io::stdin().read_to_end(&mut bytes).unwrap();
            std::io::stdout().write_all(&bytes).unwrap();
        }
        "fail" => {
            print!("partial response");
            std::process::exit(17);
        }
        "flood" => loop {
            if std::io::stdout().write_all(&[b'x'; 4096]).is_err() {
                break;
            }
        },
        "stdin-closed" => {
            let mut byte = [0u8];
            let n = std::io::stdin().read(&mut byte).unwrap();
            println!("{n}");
        }
        "start-independent" => {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["delayed-write", &args[2]])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            println!("started {}", child.id());
            std::io::stdout().flush().unwrap();
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            std::thread::sleep(Duration::from_secs(10));
        }
        "delayed-write" => {
            std::thread::sleep(Duration::from_millis(300));
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&args[2])
                .unwrap();
            file.write_all(b"one detached execution completed").unwrap();
            file.sync_all().unwrap();
        }
        _ => panic!("unknown synthetic action"),
    }
}
