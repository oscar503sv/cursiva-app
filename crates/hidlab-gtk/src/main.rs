//! Cursiva: the GNOME app that configures the mouse through hid-labd. It
//! contains no device logic; everything goes through the daemon's D-Bus
//! API (`hidlab-api`).

mod changes;
mod dbus;
mod geometry;
mod i18n;
mod model;
mod mouse;
mod pointer;
mod window;

use adw::prelude::*;
use gtk::glib;

pub const APP_ID: &str = "dev.ozkkar.Cursiva";

fn main() -> glib::ExitCode {
    i18n::init();
    glib::set_application_name("Cursiva");
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(|app| match app.active_window() {
        Some(window) => window.present(),
        None => window::build(app).present(),
    });
    app.run()
}
