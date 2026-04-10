use std::{
    env,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use tauri::{AppHandle, Manager};

const DEV_SIDECAR_NAME: &str = "imagediction-sidecar-aarch64-apple-darwin";
const BUNDLED_SIDECAR_NAME: &str = "imagediction-sidecar";
const AUDIO_READY_TIMEOUT: Duration = Duration::from_secs(3);
const AUDIO_READY_POLL_INTERVAL: Duration = Duration::from_millis(50);

fn resolve_ffmpeg_path() -> Option<PathBuf> {
    [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists())
}

pub fn capture_interactive(app: &AppHandle, output: &Path) -> Result<bool> {
    let _ = app;

    let command = if Path::new("/usr/sbin/screencapture").exists() {
        "/usr/sbin/screencapture"
    } else {
        "screencapture"
    };

    let command_output = Command::new(command)
        .arg("-i")
        .arg("-x")
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .context("failed to launch macOS partial screenshot tool")?;

    if command_output.status.success() {
        return Ok(output.exists());
    }

    let stderr = String::from_utf8_lossy(&command_output.stderr)
        .trim()
        .to_string();
    if stderr.is_empty() {
        Err(anyhow!(
            "partial screenshot tool exited with status {}",
            command_output.status
        ))
    } else {
        Err(anyhow!(stderr))
    }
}

pub fn start_recording(app: &AppHandle, output: &Path) -> Result<Child> {
    if let Some(ffmpeg) = resolve_ffmpeg_path() {
        let child = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostats",
                "-progress",
                "pipe:1",
                "-f",
                "avfoundation",
                "-thread_queue_size",
                "512",
                "-i",
                ":default",
                "-vn",
                "-ac",
                "1",
                "-ar",
                "16000",
                "-c:a",
                "pcm_s16le",
                "-y",
            ])
            .arg(output)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to launch ffmpeg recorder")?;

        return wait_for_ffmpeg_ready(child);
    }

    let child = Command::new(resolve_sidecar_path(app)?)
        .arg("record")
        .arg("--output")
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to launch recording sidecar")?;

    wait_for_sidecar_ready(child)
}

pub fn stop_recording(mut child: Child) -> Result<()> {
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }
    let output = child
        .wait_with_output()
        .context("failed waiting for recording sidecar to exit")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(anyhow!(
            "recording sidecar exited with status {}",
            output.status
        ))
    } else {
        Err(anyhow!(stderr))
    }
}

fn wait_for_ffmpeg_ready(mut child: Child) -> Result<Child> {
    let stdout = child
        .stdout
        .take()
        .context("ffmpeg recorder did not expose a progress stream")?;
    let (sender, receiver) = mpsc::channel::<String>();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(value) => {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let deadline = Instant::now() + AUDIO_READY_TIMEOUT;
    while Instant::now() < deadline {
        if child
            .try_wait()
            .context("failed to poll recording process")?
            .is_some()
        {
            return early_exit_error(child);
        }

        match receiver.recv_timeout(AUDIO_READY_POLL_INTERVAL) {
            Ok(line) => {
                if ffmpeg_progress_indicates_audio(&line) {
                    thread::sleep(Duration::from_millis(80));
                    return Ok(child);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {}
        }
    }

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }

    let output = child
        .wait_with_output()
        .context("failed waiting for recording process after startup timeout")?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(anyhow!(
            "microphone input did not begin streaming within {} seconds",
            AUDIO_READY_TIMEOUT.as_secs()
        ))
    } else {
        Err(anyhow!(stderr))
    }
}

fn wait_for_sidecar_ready(mut child: Child) -> Result<Child> {
    thread::sleep(Duration::from_millis(250));
    if child
        .try_wait()
        .context("failed to poll recording sidecar during startup")?
        .is_some()
    {
        return early_exit_error(child);
    }

    Ok(child)
}

fn ffmpeg_progress_indicates_audio(line: &str) -> bool {
    line.strip_prefix("out_time_ms=")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .is_some_and(|value| value > 0)
}

fn early_exit_error(child: Child) -> Result<Child> {
    let output = child
        .wait_with_output()
        .context("failed waiting for recording process after early exit")?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(anyhow!(
            "recording process exited before microphone audio became available"
        ))
    } else {
        Err(anyhow!(stderr))
    }
}

fn resolve_sidecar_path(app: &AppHandle) -> Result<PathBuf> {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let bundled_path = dir.join(BUNDLED_SIDECAR_NAME);
            if bundled_path.exists() {
                return Ok(bundled_path);
            }

            let dev_style_path = dir.join(DEV_SIDECAR_NAME);
            if dev_style_path.exists() {
                return Ok(dev_style_path);
            }
        }
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled_path = resource_dir.join(BUNDLED_SIDECAR_NAME);
        if bundled_path.exists() {
            return Ok(bundled_path);
        }

        let bundled_path = resource_dir
            .join("..")
            .join("MacOS")
            .join(BUNDLED_SIDECAR_NAME);
        if bundled_path.exists() {
            return Ok(bundled_path);
        }

        let dev_style_path = resource_dir.join(DEV_SIDECAR_NAME);
        if dev_style_path.exists() {
            return Ok(dev_style_path);
        }
    }

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").context("missing CARGO_MANIFEST_DIR")?);
    let dev_path = manifest_dir.join("binaries").join(DEV_SIDECAR_NAME);
    if dev_path.exists() {
        return Ok(dev_path);
    }

    Err(anyhow!(
        "Feedback sidecar was not found in resources or src-tauri/binaries"
    ))
}
