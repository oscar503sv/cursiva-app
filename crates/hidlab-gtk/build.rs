//! Compiles `po/*.po` into `$OUT_DIR/locale/<lang>/LC_MESSAGES/cursiva.mo`
//! with `msgfmt` (gettext), so `cargo run` shows translations. Without
//! msgfmt the app builds and runs in English.

use std::path::Path;
use std::process::Command;

fn main() {
    let po_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../po");
    let out = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("locale");
    println!("cargo:rerun-if-changed={}", po_dir.display());
    println!("cargo:rustc-env=CURSIVA_BUILD_LOCALEDIR={}", out.display());
    println!("cargo:rerun-if-env-changed=CURSIVA_LOCALEDIR");

    let Ok(entries) = std::fs::read_dir(&po_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "po") {
            continue;
        }
        println!("cargo:rerun-if-changed={}", path.display());
        let lang = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let dir = out.join(lang).join("LC_MESSAGES");
        std::fs::create_dir_all(&dir).expect("cannot create the locale directory");
        let status = Command::new("msgfmt")
            .arg("--check")
            .arg("-o")
            .arg(dir.join("cursiva.mo"))
            .arg(&path)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => panic!("msgfmt failed on {} ({s})", path.display()),
            Err(e) => {
                println!(
                    "cargo:warning=msgfmt not found ({e}); the app will be in English. \
                     Install it with `sudo dnf install gettext`"
                );
                return;
            }
        }
    }
}
