#[cfg(not(debug_assertions))]
use std::{
    io::{BufRead, BufReader},
    process::Command,
};

#[cfg(not(debug_assertions))]
macro_rules! p {
    ($($tokens: tt)*) => {
        println!("cargo:warning={}", format!($($tokens)*))
    }
}

fn main() {
    #[cfg(not(debug_assertions))]
    {
        p!("Build frontend");
        let output = Command::new("npm")
            .args(["run", "build"])
            .current_dir("../frontend")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .and_then(|mut child| {
                let stdout = child.stdout.take().expect("Failed to capture stdout");
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    p!("{}", line?);
                }
                child.wait_with_output()
            })
            .expect("Failed to execute command");

        if !output.status.success() {
            panic!("Command executed with failing error code");
        }
    }
}
