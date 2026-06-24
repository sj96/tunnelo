//! Privileged operations — hosts file writes with optional UAC elevation (Windows).

use anyhow::{bail, Context, Result};
#[cfg(windows)]
use std::path::{Path, PathBuf};

use crate::hosts::{hosts_path, hosts_path_elevated, managed_entries_match};

/// Write the hosts file, elevating on Windows when direct write fails.
pub fn write_hosts_file(content: &str) -> Result<()> {
    if try_direct_write(content).is_ok() {
        return Ok(());
    }
    #[cfg(windows)]
    {
        return elevated_write_hosts_windows(content);
    }
    #[cfg(not(windows))]
    {
        try_direct_write(content).context(
            "writing hosts file (try running with elevated permissions or edit /etc/hosts manually)",
        )
    }
}

fn try_direct_write(content: &str) -> Result<()> {
    let path = hosts_path();
    std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}

/// Windows exit code when the user cancels a UAC prompt (`ERROR_CANCELLED`).
#[cfg(windows)]
const WIN32_ERROR_CANCELLED: i32 = 1223;

#[cfg(windows)]
fn is_process_elevated() -> bool {
    std::process::Command::new("net")
        .args(["session"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn ps_single_quoted(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// User-writable log path (readable after elevation).
#[cfg(windows)]
fn elevate_log_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\Temp"));
    base.join("Tunnelo").join("elevate.log")
}

#[cfg(windows)]
fn powershell_64() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// Prevent a visible console window when spawning PowerShell from this process.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
fn hidden_powershell_command(ps64: &Path) -> std::process::Command {
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new(ps64);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(windows)]
fn staging_dir() -> PathBuf {
    PathBuf::from(r"C:\Windows\Temp\Tunnelo")
}

#[cfg(windows)]
struct ElevateArtifacts {
    elevated_script: PathBuf,
    launch_script: PathBuf,
    log_file: PathBuf,
}

#[cfg(windows)]
impl ElevateArtifacts {
    fn new() -> Result<Self> {
        let dir = staging_dir();
        std::fs::create_dir_all(&dir).context("creating elevation staging directory")?;
        let id = uuid::Uuid::new_v4();
        Ok(Self {
            elevated_script: dir.join(format!("hosts-{id}-elevate.ps1")),
            launch_script: dir.join(format!("hosts-{id}-launch.ps1")),
            log_file: elevate_log_path(),
        })
    }

    fn cleanup_scripts(&self) {
        let _ = std::fs::remove_file(&self.elevated_script);
        let _ = std::fs::remove_file(&self.launch_script);
    }
}

#[cfg(windows)]
fn hosts_write_succeeded(dest: &Path, content: &str) -> Result<bool> {
    if managed_entries_match(dest, content)? {
        return Ok(true);
    }
    let expected = normalize_hosts_text(content);
    if expected.is_empty() {
        let actual = std::fs::read_to_string(dest)
            .with_context(|| format!("reading {}", dest.display()))?;
        let actual = actual.strip_prefix('\u{feff}').unwrap_or(&actual);
        return Ok(normalize_hosts_text(actual).is_empty());
    }
    Ok(false)
}

#[cfg(windows)]
fn normalize_hosts_text(s: &str) -> String {
    s.replace("\r\n", "\n").trim_end_matches('\n').to_string()
}

/// Elevated script: decode embedded base64 hosts content and write to the real System32 path.
#[cfg(windows)]
fn build_elevated_script(log: &str, content_b64: &str, dest: &str) -> String {
    [
        "$ErrorActionPreference = 'Stop'",
        &format!("$log = {}", ps_single_quoted(log)),
        "function Log([string]$msg) {",
        "  $dir = Split-Path -Parent $log",
        "  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }",
        "  Add-Content -LiteralPath $log -Value $msg -Encoding UTF8",
        "}",
        "try {",
        &format!("  $dest = {}", ps_single_quoted(dest)),
        &format!("  $b64 = {}", ps_single_quoted(content_b64)),
        "  Log 'elevated script started'",
        "  $bytes = [Convert]::FromBase64String($b64)",
        "  $text = [Text.Encoding]::UTF8.GetString($bytes)",
        "  if (Test-Path -LiteralPath $dest) {",
        "    $item = Get-Item -LiteralPath $dest -Force",
        "    if ($item.IsReadOnly) { $item.IsReadOnly = $false }",
        "  }",
        "  $utf8 = New-Object System.Text.UTF8Encoding $false",
        "  [System.IO.File]::WriteAllText($dest, $text, $utf8)",
        "  Log 'wrote hosts file'",
        "  exit 0",
        "} catch {",
        "  Log (\"error: \" + $_.Exception.Message)",
        "  if ($_.Exception.InnerException) { Log $_.Exception.InnerException.Message }",
        "  exit 1",
        "}",
    ]
    .join("\n")
}

#[cfg(windows)]
fn build_launch_script(ps64: &str, elevated: &str) -> String {
    [
        "$ErrorActionPreference = 'Stop'",
        &format!("$ps64 = {}", ps_single_quoted(ps64)),
        &format!("$elevated = {}", ps_single_quoted(elevated)),
        "try {",
        "  $proc = Start-Process -FilePath $ps64 -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @(",
        "    '-NoProfile',",
        "    '-NonInteractive',",
        "    '-WindowStyle', 'Hidden',",
        "    '-ExecutionPolicy', 'Bypass',",
        "    '-File', $elevated",
        "  )",
        "  if ($null -eq $proc) {",
        &format!("    exit {WIN32_ERROR_CANCELLED}"),
        "  }",
        "  exit $proc.ExitCode",
        "} catch {",
        "  $msg = $_.Exception.Message",
        "  if ($msg -match 'canceled by the user|operation was cancelled|1223') {",
        &format!("    exit {WIN32_ERROR_CANCELLED}"),
        "  }",
        "  Write-Error $msg",
        "  exit 1",
        "}",
    ]
    .join("\n")
}

#[cfg(windows)]
fn read_tail_log(log: &Path) -> String {
    std::fs::read_to_string(log).unwrap_or_default().lines().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n")
}

#[cfg(windows)]
fn elevated_write_hosts_windows(content: &str) -> Result<()> {
    if is_process_elevated() {
        let dest = hosts_path_elevated();
        std::fs::write(&dest, content).with_context(|| {
            format!(
                "writing hosts file as administrator ({}); check antivirus or group policy",
                dest.display()
            )
        })?;
        return Ok(());
    }

    let artifacts = ElevateArtifacts::new()?;
    let dest = hosts_path_elevated();
    let ps64 = powershell_64();

    if !ps64.exists() {
        bail!("PowerShell not found at {}", ps64.display());
    }

    let content_b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content.as_bytes());

    // Clear previous log so we only see this attempt.
    if let Some(parent) = artifacts.log_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&artifacts.log_file, "");

    let elevated_script = build_elevated_script(
        &artifacts.log_file.to_string_lossy(),
        &content_b64,
        &dest.to_string_lossy(),
    );
    std::fs::write(&artifacts.elevated_script, elevated_script)
        .context("writing elevation script")?;

    let launch_script = build_launch_script(
        &ps64.to_string_lossy(),
        &artifacts.elevated_script.to_string_lossy(),
    );
    std::fs::write(&artifacts.launch_script, launch_script)
        .context("writing elevation launcher script")?;

    let output = hidden_powershell_command(&ps64)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            artifacts
                .launch_script
                .to_str()
                .context("launch script path is not valid UTF-8")?,
        ])
        .output()
        .context("spawning PowerShell elevation launcher")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let log_tail = read_tail_log(&artifacts.log_file);
    let launcher_stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let launcher_stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    let verify_path = hosts_path();
    let succeeded = hosts_write_succeeded(&verify_path, content)?;
    artifacts.cleanup_scripts();

    if succeeded {
        return Ok(());
    }

    if exit_code == WIN32_ERROR_CANCELLED {
        bail!("elevated hosts update failed: UAC prompt was denied or cancelled");
    }

    let mut parts = Vec::new();
    if !log_tail.is_empty() {
        parts.push(log_tail);
    }
    if !launcher_stderr.is_empty() {
        parts.push(format!("launcher stderr: {launcher_stderr}"));
    }
    if !launcher_stdout.is_empty() {
        parts.push(format!("launcher stdout: {launcher_stdout}"));
    }
    let detail = parts.join("; ");

    if !output.status.success() {
        if detail.is_empty() {
            bail!(
                "elevated hosts update failed (exit code {exit_code}). \
                 Approve the UAC prompt and ensure nothing blocks edits to the hosts file."
            );
        }
        bail!("elevated hosts update failed (exit code {exit_code}): {detail}");
    }

    if detail.is_empty() {
        bail!(
            "elevated hosts update failed: hosts file was not updated. \
             Check {} for details or edit the hosts file manually as administrator.",
            artifacts.log_file.display()
        );
    }
    bail!("elevated hosts update failed: hosts file was not updated: {detail}");
}
