//! Native adversarial process fixture; no semantic engine or claimed runtime identity.
use std::{env, io::Write, process::Command, time::Duration};
fn main() {
    let mode = std::path::PathBuf::from(env::args_os().next().unwrap())
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    if mode.contains("parent") {
        Command::new(env::current_exe().unwrap().with_file_name("worker-sleep"))
            .spawn()
            .unwrap();
        return;
    }
    if mode.contains("sleep") {
        std::thread::sleep(Duration::from_secs(10));
        return;
    }
    if mode.contains("stderr") {
        std::io::stderr().write_all(b"123456789").unwrap();
        return;
    }
    if mode.contains("malformed") {
        print!("not JSON");
        return;
    }
    if mode.contains("exit37") {
        print!("native fixture prior output\n");
        eprint!("fixture failure\n");
        std::process::exit(37)
    }
    print!("123456789")
}
