use std::process::Command;
use std::time::Instant;

pub struct ExecResult{ pub exit_code:i32, pub stdout:String, pub stderr:String, pub duration_ms:u128 }

pub fn run(command:&str, args:&[&str], workdir:&str, timeout_secs:u64)->ExecResult{
    let start = Instant::now();
    let out = Command::new(command).args(args).current_dir(workdir).output().expect("run");
    let dur = start.elapsed().as_millis();
    ExecResult{ exit_code: out.status.code().unwrap_or(-1), stdout: String::from_utf8_lossy(&out.stdout).to_string(), stderr: String::from_utf8_lossy(&out.stderr).to_string(), duration_ms: dur }
}
