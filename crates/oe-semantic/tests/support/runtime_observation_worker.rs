//! Native process fixture: payload is untrusted historical test data, not an installed-runtime identity.
use std::{env, fs, io::Write, process::Command, time::Duration};
fn main() {
    let exe = env::current_exe().unwrap();
    let name = exe.file_name().unwrap().to_string_lossy();
    if name.contains("parent") {
        Command::new(exe.with_file_name("worker-sleep"))
            .spawn()
            .unwrap();
        return;
    }
    if name.contains("sleep") {
        std::thread::sleep(Duration::from_secs(10));
        return;
    }
    if name.contains("stderr") {
        std::io::stderr().write_all(b"123456789").unwrap();
        return;
    }
    if name.contains("overflow") {
        print!("123456789");
        return;
    }
    if name.contains("malformed") {
        print!("not JSON");
        return;
    }
    if name.contains("logging") {
        fs::write(
            "var/observed.args",
            env::args().skip(1).collect::<Vec<_>>().join("\n") + "\n",
        )
        .unwrap();
        fs::write(
            "var/observed.cwd",
            env::current_dir().unwrap().to_string_lossy().as_bytes(),
        )
        .unwrap();
    }
    std::io::stdout()
        .write_all(&fs::read("var/fixture-observation.json").unwrap())
        .unwrap();
}
