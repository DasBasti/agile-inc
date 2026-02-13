use std::process::{Command, Stdio};
use std::time::Instant;
use thiserror::Error;

pub use agile_common::types::{DiffStat, ExecJob, ExecResult, OutputLimits};

#[derive(Error, Debug)]
pub enum RunnerError {
    #[error("execution error: {0}")]
    Execution(String),
    #[error("timeout after {0} seconds")]
    Timeout(u64),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type RunnerResult<T> = Result<T, RunnerError>;

fn truncate(s: &str, max_bytes: usize) -> String {
    let mut truncated = s.to_string();
    if truncated.len() > max_bytes * 1024 {
        truncated.truncate(max_bytes * 1024);
        truncated.push_str(&format!("\n\n[Output truncated to {}KB]", max_bytes));
    }
    truncated
}

pub async fn run(job: &ExecJob) -> RunnerResult<ExecResult> {
    let start = Instant::now();

    let mut cmd = Command::new(&job.command);
    cmd.args(&job.args)
        .current_dir(&job.workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in &job.env {
        cmd.env(key, value);
    }

    let output = cmd.output().map_err(|e| RunnerError::Execution(e.to_string()))?;

    let duration_ms = start.elapsed().as_millis();

    let stdout = truncate(
        &String::from_utf8_lossy(&output.stdout).to_string(),
        job.output_limits.max_stdout_kb as usize,
    );
    let stderr = truncate(
        &String::from_utf8_lossy(&output.stderr).to_string(),
        job.output_limits.max_stderr_kb as usize,
    );

    let artifacts = vec![];

    Ok(ExecResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        duration_ms,
        artifacts,
    })
}

pub async fn diffstat(workdir: &str) -> RunnerResult<DiffStat> {
    let output = Command::new("git")
        .args(["diff", "--stat", "--numstat"])
        .current_dir(workdir)
        .output()
        .map_err(|e| RunnerError::Execution(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(RunnerError::Execution(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files_changed: u32 = 0;
    let mut insertions: u32 = 0;
    let mut deletions: u32 = 0;
    let mut files: Vec<String> = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            let file = parts[2].to_string();
            let ins = parts[0].parse::<u32>().unwrap_or(0);
            let del = parts[1].parse::<u32>().unwrap_or(0);

            if ins > 0 || del > 0 {
                files_changed += 1;
                insertions += ins;
                deletions += del;
                files.push(file);
            }
        }
    }

    Ok(DiffStat {
        files_changed,
        insertions,
        deletions,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diffstat() {
        let result = diffstat(".").await;
        if result.is_ok() {
            let stat = result.unwrap();
            println!(
                "files: {}, insertions: {}, deletions: {}",
                stat.files_changed, stat.insertions, stat.deletions
            );
        }
    }

    #[tokio::test]
    async fn test_exec_with_timeout() {
        let job = ExecJob {
            command: "sleep".to_string(),
            args: vec!["0.1".to_string()],
            workdir: ".".to_string(),
            timeout_seconds: 5,
            output_limits: OutputLimits::default(),
            ..Default::default()
        };

        let result = run(&job).await.unwrap();
        assert_eq!(result.exit_code, 0);
    }
}
