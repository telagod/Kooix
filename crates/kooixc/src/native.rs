use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{self, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::Diagnostic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum NativeError {
    Diagnostics(Vec<Diagnostic>),
    Io(std::io::Error),
    ToolNotFound(&'static str),
    CommandFailed { tool: &'static str, stderr: String },
    TimedOut { timeout_ms: u64 },
}

impl fmt::Display for NativeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeError::Diagnostics(diagnostics) => {
                write!(
                    f,
                    "semantic checks failed with {} diagnostic(s)",
                    diagnostics.len()
                )
            }
            NativeError::Io(error) => write!(f, "io error: {error}"),
            NativeError::ToolNotFound(tool) => {
                write!(f, "required tool '{tool}' not found in PATH")
            }
            NativeError::CommandFailed { tool, stderr } => {
                write!(f, "{tool} failed: {}", stderr.trim())
            }
            NativeError::TimedOut { timeout_ms } => {
                write!(f, "process timed out after {timeout_ms} ms")
            }
        }
    }
}

impl std::error::Error for NativeError {}

impl From<std::io::Error> for NativeError {
    fn from(value: std::io::Error) -> Self {
        NativeError::Io(value)
    }
}

pub fn compile_llvm_ir_to_executable(ir: &str, output_path: &Path) -> Result<(), NativeError> {
    compile_llvm_ir_to_executable_with_tools(ir, output_path, "llc", "clang")
}

pub fn run_executable(path: &Path) -> Result<RunOutput, NativeError> {
    run_executable_with_args(path, &[])
}

pub fn run_executable_with_args(path: &Path, args: &[String]) -> Result<RunOutput, NativeError> {
    run_executable_with_args_and_stdin(path, args, None)
}

pub fn run_executable_with_args_and_stdin(
    path: &Path,
    args: &[String],
    stdin_data: Option<&[u8]>,
) -> Result<RunOutput, NativeError> {
    run_executable_with_args_and_stdin_and_timeout(path, args, stdin_data, None)
}

pub fn run_executable_with_args_and_stdin_and_timeout(
    path: &Path,
    args: &[String],
    stdin_data: Option<&[u8]>,
    timeout_ms: Option<u64>,
) -> Result<RunOutput, NativeError> {
    let mut command = Command::new(path);
    command.args(args);

    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command.spawn().map_err(NativeError::Io)?;
    if let Some(data) = stdin_data {
        let Some(mut stdin) = child.stdin.take() else {
            return Err(NativeError::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "failed to open child stdin",
            )));
        };
        stdin.write_all(data).map_err(NativeError::Io)?;
    }

    if let Some(timeout_ms) = timeout_ms {
        let timeout = Duration::from_millis(timeout_ms);
        let deadline = Instant::now() + timeout;
        let poll_interval = Duration::from_millis(5);

        loop {
            if child.try_wait().map_err(NativeError::Io)?.is_some() {
                break;
            }

            let now = Instant::now();
            if now >= deadline {
                if child.try_wait().map_err(NativeError::Io)?.is_some() {
                    break;
                }

                if let Err(kill_err) = child.kill() {
                    if child.try_wait().map_err(NativeError::Io)?.is_none() {
                        return Err(NativeError::Io(kill_err));
                    }
                }

                let _ = child.wait();
                return Err(NativeError::TimedOut { timeout_ms });
            }

            let remaining = deadline.saturating_duration_since(now);
            thread::sleep(std::cmp::min(poll_interval, remaining));
        }
    }

    let output = child.wait_with_output().map_err(NativeError::Io)?;
    Ok(RunOutput {
        status_code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

pub fn compile_llvm_ir_to_executable_with_tools(
    ir: &str,
    output_path: &Path,
    llc_tool: &'static str,
    clang_tool: &'static str,
) -> Result<(), NativeError> {
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let temp_dir = create_temp_workdir()?;
    let ll_path = temp_dir.join("module.ll");
    let obj_path = temp_dir.join("module.o");

    fs::write(&ll_path, ir)?;

    let ll_path_string = ll_path.to_string_lossy().to_string();
    let obj_path_string = obj_path.to_string_lossy().to_string();
    run_command(
        llc_tool,
        &[
            "-filetype=obj",
            ll_path_string.as_str(),
            "-o",
            obj_path_string.as_str(),
        ],
    )?;

    let output_path_string = output_path.to_string_lossy().to_string();
    run_command(
        clang_tool,
        &[obj_path_string.as_str(), "-o", output_path_string.as_str()],
    )?;

    let _ = fs::remove_dir_all(&temp_dir);
    Ok(())
}

fn run_command(tool: &'static str, args: &[&str]) -> Result<(), NativeError> {
    let output = match Command::new(tool).args(args).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(NativeError::ToolNotFound(tool));
        }
        Err(error) => return Err(NativeError::Io(error)),
    };

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Err(NativeError::CommandFailed { tool, stderr })
}

fn create_temp_workdir() -> Result<std::path::PathBuf, NativeError> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();

    let path = std::env::temp_dir().join(format!("kooixc-native-{}-{nanos}", process::id()));
    fs::create_dir_all(&path)?;
    Ok(path)
}
