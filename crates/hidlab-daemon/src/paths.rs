//! XDG locations and atomic file writes.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

fn xdg(var: &str, fallback: &str) -> Result<PathBuf, String> {
    if let Some(v) = std::env::var_os(var).filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(v));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| format!("neither {var} nor HOME is set"))?;
    Ok(PathBuf::from(home).join(fallback))
}

/// `$XDG_CONFIG_HOME/hid-lab/config.toml`, default `~/.config/hid-lab/config.toml`.
pub fn config_file() -> Result<PathBuf, String> {
    Ok(xdg("XDG_CONFIG_HOME", ".config")?.join("hid-lab/config.toml"))
}

/// `$XDG_STATE_HOME/hid-lab/daemon/state.toml`, default under `~/.local/state`.
pub fn state_file() -> Result<PathBuf, String> {
    Ok(xdg("XDG_STATE_HOME", ".local/state")?.join("hid-lab/daemon/state.toml"))
}

/// `path` with the home directory shown as `~`.
pub fn tilde(path: &Path) -> String {
    match std::env::var_os("HOME") {
        Some(h) if !h.is_empty() => match path.strip_prefix(&h) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        _ => path.display().to_string(),
    }
}

/// Writes `text` to a temporary file next to `path`, then renames it, so a
/// crash never leaves a half-written file.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", tilde(dir)))?;
    let tmp = path.with_extension("toml.tmp");
    let write = || -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    };
    write().map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot write {}: {e}", tilde(path))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_the_file() {
        let dir = std::env::temp_dir().join(format!("hidlab-paths-{}", std::process::id()));
        let file = dir.join("sub/x.toml");
        write_atomic(&file, "a = 1\n").unwrap();
        write_atomic(&file, "a = 2\n").unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "a = 2\n");
        assert!(!file.with_extension("toml.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
