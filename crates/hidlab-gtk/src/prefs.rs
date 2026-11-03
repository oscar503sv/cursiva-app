//! Preferences: start at login and troubleshooting (service, last apply,
//! export diagnostics). The dialog shows the status read when it opened.

use std::cell::Cell;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use hidlab_api::daemon::Status;

use crate::dbus;
use crate::model::{fill, last_apply_text};
use crate::problems::{System, app_report};

/// The daemon's diagnostics, or the app's own short report when the
/// daemon cannot answer.
pub async fn diagnostics_text(conn: Option<gio::DBusConnection>) -> String {
    let result = match conn {
        Some(c) => dbus::diagnostics(&c).await,
        None => Err(gettext("No connection to the session bus")),
    };
    result.unwrap_or_else(|e| app_report(&e, &System::current()))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn show(
    window: &adw::ApplicationWindow,
    conn: Option<gio::DBusConnection>,
    status: Option<&Status>,
) {
    let dialog = adw::PreferencesDialog::builder()
        .search_enabled(false)
        .build();
    let page = adw::PreferencesPage::new();
    dialog.add(&page);

    let startup = adw::PreferencesGroup::new();
    let login = adw::SwitchRow::builder()
        .title(gettext("Apply my settings when I log in"))
        .subtitle(gettext("Works without opening Cursiva"))
        .build();
    let known = status.and_then(|s| s.start_at_login);
    login.set_active(known == Some(true));
    login.set_sensitive(known.is_some() && conn.is_some());
    if status.is_some() && known.is_none() {
        login.set_subtitle(&gettext(
            "Cannot tell whether the service starts at login: systemd did not answer",
        ));
    }
    startup.add(&login);
    page.add(&startup);

    let export_conn = conn.clone();
    let syncing = Rc::new(Cell::new(false));
    let d = dialog.downgrade();
    login.connect_active_notify(move |row| {
        if syncing.get() {
            return;
        }
        let Some(conn) = conn.clone() else { return };
        let on = row.is_active();
        row.set_sensitive(false);
        let (row, syncing, d) = (row.clone(), syncing.clone(), d.clone());
        glib::spawn_future_local(async move {
            let result = dbus::set_start_at_login(&conn, on).await;
            let state = match &result {
                Ok(Some(v)) => *v,
                _ => !on,
            };
            syncing.set(true);
            row.set_active(state);
            syncing.set(false);
            row.set_sensitive(true);
            let error = match result {
                Ok(Some(v)) if v == on => None,
                Ok(_) => Some(gettext("systemd did not change the setting")),
                Err(e) => Some(e),
            };
            if let (Some(e), Some(d)) = (error, d.upgrade()) {
                d.add_toast(adw::Toast::builder().title(e).timeout(0).build());
            }
        });
    });

    let trouble = adw::PreferencesGroup::builder()
        .title(gettext("Troubleshooting"))
        .build();
    let service = adw::ActionRow::builder()
        .title(gettext("Service"))
        .subtitle(match status {
            // TRANSLATORS: {version} is the service's version, such as 0.1.0.
            Some(s) => fill(
                &gettext("Running · hid-labd {version}"),
                &[("version", &s.daemon_version)],
            ),
            None => gettext("Not running"),
        })
        .subtitle_selectable(true)
        .build();
    trouble.add(&service);
    if let Some(s) = status {
        let last = adw::ActionRow::builder()
            .title(gettext("Last time your settings were applied"))
            .subtitle(last_apply_text(s.last_apply.as_ref(), now_unix()))
            .build();
        trouble.add(&last);
    }
    let export = adw::ActionRow::builder()
        .title(gettext("Export Diagnostics"))
        .subtitle(gettext(
            "A text report to attach to a bug report. Your user name, computer name and Bluetooth addresses are left out.",
        ))
        .activatable(true)
        .build();
    export.add_suffix(&gtk::Image::from_icon_name("document-save-symbolic"));
    trouble.add(&export);
    page.add(&trouble);

    let (w, d) = (window.downgrade(), dialog.downgrade());
    let conn = export_conn;
    export.connect_activated(move |_| {
        let (w, d, conn) = (w.clone(), d.clone(), conn.clone());
        glib::spawn_future_local(async move {
            let Some(window) = w.upgrade() else { return };
            let message = match save_diagnostics(&window, conn).await {
                Ok(Some(name)) => fill(&gettext("Diagnostics saved to {file}"), &[("file", &name)]),
                Ok(None) => return,
                Err(e) => e,
            };
            if let Some(d) = d.upgrade() {
                d.add_toast(adw::Toast::new(&message));
            }
        });
    });
    dialog.present(Some(window));
}

/// Asks where to save, then writes the report. `Ok(None)` when the user
/// cancelled.
async fn save_diagnostics(
    window: &adw::ApplicationWindow,
    conn: Option<gio::DBusConnection>,
) -> Result<Option<String>, String> {
    let chooser = gtk::FileDialog::builder()
        .title(gettext("Export Diagnostics"))
        .initial_name("cursiva-diagnostics.txt")
        .modal(true)
        .build();
    let file = match chooser.save_future(Some(window)).await {
        Ok(f) => f,
        Err(e)
            if e.matches(gtk::DialogError::Dismissed) || e.matches(gtk::DialogError::Cancelled) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e.message().to_owned()),
    };
    let text = diagnostics_text(conn).await;
    file.replace_contents_future(
        text.into_bytes(),
        None,
        false,
        gio::FileCreateFlags::REPLACE_DESTINATION,
    )
    .await
    .map_err(|(_, e)| {
        fill(
            // TRANSLATORS: {error} is the system's message.
            &gettext("Cannot save the diagnostics: {error}"),
            &[("error", e.message())],
        )
    })?;
    Ok(Some(file.basename().map_or_else(String::new, |p| {
        p.to_string_lossy().into_owned()
    })))
}
