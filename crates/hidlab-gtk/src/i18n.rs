//! gettext setup. Translations live in `po/` (domain `cursiva`).
//!
//! Where the compiled catalogs are looked up, first match wins:
//! 1. `CURSIVA_LOCALEDIR` in the environment (testing a translation);
//! 2. `CURSIVA_LOCALEDIR` at build time (packages set `/usr/share/locale`);
//! 3. the catalogs `build.rs` compiled into the build directory, so
//!    `cargo run` is translated without installing anything.

use std::path::PathBuf;

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

pub const DOMAIN: &str = "cursiva";

pub fn init() {
    // SAFETY: called first thing in `main`, before GTK or any other thread
    // starts, as gettext-rs requires.
    unsafe {
        setlocale(LocaleCategory::LcAll, "");
    }
    let dir = std::env::var_os("CURSIVA_LOCALEDIR")
        .map(PathBuf::from)
        .or_else(|| option_env!("CURSIVA_LOCALEDIR").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(env!("CURSIVA_BUILD_LOCALEDIR")));
    // A failure leaves the English text, which is still usable.
    let _ = bindtextdomain(DOMAIN, dir);
    let _ = bind_textdomain_codeset(DOMAIN, "UTF-8");
    let _ = textdomain(DOMAIN);
}
