//! Detects other mouse configurators (requirements §6): two programs
//! configuring one device give unpredictable results.

use std::fs;
use std::path::Path;

/// Program names checked, as they appear in `/proc/<pid>/comm` or as the
/// first or second command-line word (Solaar is a Python script).
const KNOWN: &[&str] = &["solaar", "logid"];

pub use hidlab_api::daemon::Conflict;

fn base(word: &[u8]) -> &[u8] {
    word.rsplit(|&b| b == b'/').next().unwrap_or(word)
}

fn matches(comm: &str, cmdline: &[u8]) -> Option<&'static str> {
    let words: Vec<&[u8]> = cmdline.split(|&b| b == 0).take(2).collect();
    KNOWN
        .iter()
        .copied()
        .find(|&k| comm.trim_end() == k || words.iter().any(|w| base(w) == k.as_bytes()))
}

/// Scans `proc_root` (normally `/proc`) for known configurators.
pub fn scan_in(proc_root: &Path) -> Vec<Conflict> {
    let Ok(dir) = fs::read_dir(proc_root) else {
        return Vec::new();
    };
    let own = std::process::id();
    let mut out: Vec<Conflict> = dir
        .flatten()
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            if pid == own {
                return None;
            }
            // Processes can exit while we look; unreadable entries are skipped.
            let comm = fs::read_to_string(e.path().join("comm")).unwrap_or_default();
            let cmdline = fs::read(e.path().join("cmdline")).unwrap_or_default();
            matches(&comm, &cmdline).map(|p| Conflict {
                program: p.to_owned(),
                pid,
            })
        })
        .collect();
    out.sort_by_key(|c| c.pid);
    out
}

pub fn scan() -> Vec<Conflict> {
    scan_in(Path::new("/proc"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_entry(root: &Path, pid: u32, comm: &str, cmdline: &[u8]) {
        let d = root.join(pid.to_string());
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("comm"), comm).unwrap();
        fs::write(d.join("cmdline"), cmdline).unwrap();
    }

    #[test]
    fn finds_solaar_and_logid_only() {
        // SYNTHETIC /proc tree.
        let root = std::env::temp_dir().join(format!("hidlab-proc-{}", std::process::id()));
        proc_entry(
            &root,
            10,
            "python3\n",
            b"/usr/bin/python3\0/usr/bin/solaar\0-w\0hide\0",
        );
        proc_entry(&root, 11, "logid\n", b"/usr/bin/logid\0");
        proc_entry(&root, 12, "bash\n", b"bash\0-c\0echo solaar\0");
        proc_entry(&root, 13, "gedit\n", b"gedit\0/home/u/solaar-notes\0");
        fs::create_dir_all(root.join("self")).unwrap();
        let found = scan_in(&root);
        assert_eq!(
            found,
            [
                Conflict {
                    program: "solaar".into(),
                    pid: 10
                },
                Conflict {
                    program: "logid".into(),
                    pid: 11
                },
            ]
        );
        assert!(found[1].advice().contains("systemctl stop logid"));
        fs::remove_dir_all(&root).unwrap();
    }
}
