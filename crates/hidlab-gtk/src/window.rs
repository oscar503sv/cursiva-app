//! The main window: a sidebar with Home, Pointer and scrolling, and
//! Buttons, and a Home page that follows the daemon live.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use hidlab_api::daemon::{Status, UNIT};

use crate::dbus::{self, Event, Subscriptions};
use crate::model::{Health, View, fill};
use crate::mouse::MouseView;

/// Width under which the sidebar folds away (the narrow mockup).
const NARROW: &str = "max-width: 600sp";

struct Page {
    name: &'static str,
    title: String,
    icon: &'static str,
}

fn pages() -> [Page; 3] {
    [
        Page {
            name: "home",
            title: gettext("Home"),
            icon: "go-home-symbolic",
        },
        Page {
            name: "pointer",
            title: gettext("Pointer and scrolling"),
            icon: "input-mouse-symbolic",
        },
        Page {
            name: "buttons",
            title: gettext("Buttons"),
            icon: "input-keyboard-symbolic",
        },
    ]
}

/// Widgets of the Home page that change with the daemon's state.
struct Home {
    states: gtk::Stack,
    no_daemon: adw::StatusPage,
    no_mouse: adw::StatusPage,
    device: adw::PreferencesGroup,
    health: adw::ActionRow,
    health_icon: gtk::Image,
    connection: gtk::Label,
    battery: adw::ActionRow,
    battery_value: gtk::Label,
    battery_icon: gtk::Image,
    dpi: gtk::Label,
    wheel: gtk::Label,
}

struct App {
    home: Home,
    conn: RefCell<Option<gio::DBusConnection>>,
    subscriptions: RefCell<Option<Subscriptions>>,
    status: RefCell<Option<Status>>,
    fetching: Cell<bool>,
    fetch_again: Cell<bool>,
    /// The daemon answered at least once; a lost name owner then means it
    /// stopped, rather than not started yet.
    seen_daemon: Cell<bool>,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Cursiva")
        .icon_name(crate::APP_ID)
        .default_width(960)
        .default_height(700)
        .width_request(360)
        .height_request(480)
        .build();

    let content_stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    let (home_widget, home) = home_page();
    let pages = pages();
    content_stack.add_titled(&home_widget, Some(pages[0].name), &pages[0].title);
    content_stack.add_titled(
        &later_page(
            pages[1].icon,
            &gettext("Pointer speed, wheel and thumb wheel settings arrive in the next update."),
        ),
        Some(pages[1].name),
        &pages[1].title,
    );
    content_stack.add_titled(
        &later_page(
            pages[2].icon,
            &gettext("Choosing what each button does arrives in a later update."),
        ),
        Some(pages[2].name),
        &pages[2].title,
    );

    let content = adw::NavigationPage::builder()
        .title(&pages[0].title)
        .tag("content")
        .child(&with_header(&content_stack, false))
        .build();

    let list = gtk::ListBox::builder()
        .css_classes(["navigation-sidebar"])
        .selection_mode(gtk::SelectionMode::Browse)
        .build();
    for page in &pages {
        let row_box = gtk::Box::builder().spacing(12).build();
        row_box.append(&gtk::Image::from_icon_name(page.icon));
        row_box.append(&gtk::Label::builder().label(&page.title).xalign(0.0).build());
        let row = gtk::ListBoxRow::builder().child(&row_box).build();
        row.set_widget_name(page.name);
        list.append(&row);
    }
    let sidebar_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    let sidebar = adw::NavigationPage::builder()
        .title("Cursiva")
        .tag("sidebar")
        .child(&with_header(&sidebar_scroll, true))
        .build();

    let split = adw::NavigationSplitView::builder()
        .sidebar(&sidebar)
        .content(&content)
        .min_sidebar_width(200.0)
        .build();
    list.connect_row_activated(glib::clone!(
        #[weak]
        split,
        #[weak]
        content_stack,
        #[weak]
        content,
        move |_, row| {
            let name = row.widget_name();
            content_stack.set_visible_child_name(&name);
            if let Some(title) = content_stack
                .page(&content_stack.visible_child().unwrap())
                .title()
            {
                content.set_title(&title);
            }
            split.set_show_content(true);
        }
    ));
    list.select_row(list.row_at_index(0).as_ref());

    let narrow =
        adw::Breakpoint::new(adw::BreakpointCondition::parse(NARROW).expect("valid condition"));
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(narrow);
    window.set_content(Some(&split));

    let state = Rc::new(App {
        home,
        conn: RefCell::new(None),
        subscriptions: RefCell::new(None),
        status: RefCell::new(None),
        fetching: Cell::new(false),
        fetch_again: Cell::new(false),
        seen_daemon: Cell::new(false),
    });
    state.render(&View::Connecting);
    let retry = Rc::downgrade(&state);
    if let Some(button) = state.home.no_daemon.child().and_downcast::<gtk::Button>() {
        button.connect_clicked(move |_| {
            if let Some(s) = retry.upgrade() {
                s.render(&View::Connecting);
                s.refresh();
            }
        });
    }
    glib::spawn_future_local(start(Rc::downgrade(&state)));
    // The window keeps the state alive; the async parts hold weak references.
    window.connect_destroy(move |_| {
        let _ = &state;
    });
    window
}

fn with_header(child: &impl IsA<gtk::Widget>, sidebar: bool) -> adw::ToolbarView {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    if sidebar {
        header.set_show_title(true);
    }
    view.add_top_bar(&header);
    view.set_content(Some(child));
    view
}

fn later_page(icon: &str, text: &str) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title(gettext("Coming soon"))
        .description(text)
        .build()
}

fn value_label() -> gtk::Label {
    gtk::Label::builder()
        .css_classes(["dim-label", "numeric"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build()
}

fn home_page() -> (gtk::Stack, Home) {
    let states = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();

    let connecting = adw::StatusPage::builder()
        .title(gettext("Connecting to the Cursiva service…"))
        .child(
            &adw::Spinner::builder()
                .width_request(32)
                .height_request(32)
                .build(),
        )
        .build();
    states.add_named(&connecting, Some("connecting"));

    let no_daemon = adw::StatusPage::builder()
        .icon_name("dialog-error-symbolic")
        .title(gettext("Cannot reach the Cursiva service"))
        .child(
            &gtk::Button::builder()
                .label(gettext("Try Again"))
                .halign(gtk::Align::Center)
                .css_classes(["pill", "suggested-action"])
                .build(),
        )
        .build();
    states.add_named(&no_daemon, Some("no-daemon"));

    let no_mouse = adw::StatusPage::builder()
        .icon_name("input-mouse-symbolic")
        .title(gettext("Connect your mouse"))
        .build();
    states.add_named(&no_mouse, Some("no-mouse"));

    let mouse = MouseView::new();
    mouse.set_halign(gtk::Align::Center);
    let hint = gtk::Label::builder()
        .label(gettext("Click a part of the mouse to change it"))
        .css_classes(["dim-label"])
        .wrap(true)
        .justify(gtk::Justification::Center)
        .build();

    let health_icon = gtk::Image::new();
    let health = adw::ActionRow::builder().subtitle_selectable(true).build();
    health.add_prefix(&health_icon);
    let status_group = adw::PreferencesGroup::new();
    status_group.add(&health);

    let row = |title: String, subtitle: Option<String>| {
        let r = adw::ActionRow::builder().title(title).build();
        if let Some(s) = subtitle {
            r.set_subtitle(&s);
        }
        let v = value_label();
        r.add_suffix(&v);
        (r, v)
    };
    let device = adw::PreferencesGroup::new();
    let (conn_row, connection) = row(gettext("Connection"), None);
    let (battery, battery_value) = row(gettext("Battery"), None);
    let battery_icon = gtk::Image::new();
    battery.add_suffix(&battery_icon);
    let (profile_row, profile) = row(
        gettext("Profile"),
        Some(gettext("The same settings in every app")),
    );
    profile.set_label(&gettext("General"));
    device.add(&conn_row);
    device.add(&battery);
    device.add(&profile_row);

    let settings = adw::PreferencesGroup::builder()
        .title(gettext("Active settings"))
        .build();
    let (dpi_row, dpi) = row(gettext("Pointer speed"), None);
    let (wheel_row, wheel) = row(gettext("Wheel"), None);
    settings.add(&dpi_row);
    settings.add(&wheel_row);

    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(12)
        .margin_end(12)
        .build();
    column.append(&mouse);
    column.append(&hint);
    column.append(&status_group);
    column.append(&device);
    column.append(&settings);
    let connected = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(
            &adw::Clamp::builder()
                .maximum_size(560)
                .child(&column)
                .build(),
        )
        .build();
    states.add_named(&connected, Some("connected"));

    let home = Home {
        states: states.clone(),
        no_daemon,
        no_mouse,
        device,
        health,
        health_icon,
        connection,
        battery,
        battery_value,
        battery_icon,
        dpi,
        wheel,
    };
    (states, home)
}

async fn start(app: Weak<App>) {
    let conn = dbus::session().await;
    let Some(state) = app.upgrade() else { return };
    let conn = match conn {
        Ok(c) => c,
        Err(e) => return state.render(&View::NoDaemon(e)),
    };
    let weak = app.clone();
    let subs = dbus::subscribe(&conn, move |event| {
        if let Some(s) = weak.upgrade() {
            s.on_event(event);
        }
    });
    *state.subscriptions.borrow_mut() = Some(subs);
    *state.conn.borrow_mut() = Some(conn);
    state.refresh();
}

impl App {
    fn on_event(self: &Rc<Self>, event: Event) {
        match event {
            Event::Changed | Event::DaemonStarted => self.refresh(),
            Event::DaemonStopped => {
                if self.seen_daemon.get() {
                    self.status.borrow_mut().take();
                    self.render(&View::NoDaemon(fill(
                        // TRANSLATORS: {unit} is hid-labd.service.
                        &gettext("The Cursiva service stopped. Start it with `systemctl --user start {unit}`"),
                        &[("unit", UNIT)],
                    )));
                }
            }
            Event::Battery(e) => {
                let view = {
                    let mut status = self.status.borrow_mut();
                    let Some(st) = status.as_mut() else { return };
                    st.battery = e.battery;
                    View::from_status(st)
                };
                self.render(&view);
            }
        }
    }

    /// Fetches the status, at most one call at a time; a request during a
    /// call runs one more call afterwards.
    fn refresh(self: &Rc<Self>) {
        if self.fetching.replace(true) {
            self.fetch_again.set(true);
            return;
        }
        let Some(conn) = self.conn.borrow().clone() else {
            self.fetching.set(false);
            return;
        };
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            loop {
                let result = dbus::status(&conn).await;
                let Some(s) = weak.upgrade() else { return };
                let view = match result {
                    Ok(st) => {
                        s.seen_daemon.set(true);
                        let v = View::from_status(&st);
                        *s.status.borrow_mut() = Some(st);
                        v
                    }
                    Err(e) => View::NoDaemon(e),
                };
                s.render(&view);
                if !s.fetch_again.replace(false) {
                    s.fetching.set(false);
                    return;
                }
            }
        });
    }

    fn render(&self, view: &View) {
        let h = &self.home;
        match view {
            View::Connecting => h.states.set_visible_child_name("connecting"),
            View::NoDaemon(err) => {
                h.no_daemon
                    .set_description(Some(&glib::markup_escape_text(err)));
                h.states.set_visible_child_name("no-daemon");
            }
            View::NoMouse { config_error } => {
                let mut text = gettext(
                    "Turn the mouse on and connect it over Bluetooth. Cursiva finds it by itself.",
                );
                if let Some(err) = config_error {
                    text.push_str("\n\n");
                    text.push_str(&fill(
                        // TRANSLATORS: {error} is the daemon's message, in English.
                        &gettext("Your settings file has an error: {error}"),
                        &[("error", err)],
                    ));
                }
                h.no_mouse
                    .set_description(Some(&glib::markup_escape_text(&text)));
                h.states.set_visible_child_name("no-mouse");
            }
            View::Connected(s) => {
                h.device.set_title(&s.name);
                h.health.set_title(&glib::markup_escape_text(&s.headline));
                h.health
                    .set_subtitle(&glib::markup_escape_text(s.detail.as_deref().unwrap_or("")));
                let (icon, class) = match s.health {
                    Health::Good => ("emblem-ok-symbolic", "success"),
                    Health::Warning => ("dialog-warning-symbolic", "warning"),
                };
                h.health_icon.set_icon_name(Some(icon));
                h.health_icon.set_css_classes(&[class]);
                h.connection.set_label(&s.connection);
                match &s.battery {
                    Some(b) => {
                        h.battery.set_subtitle(&glib::markup_escape_text(&b.status));
                        h.battery_value.set_label(&b.value);
                        h.battery_icon.set_icon_name(Some(&b.icon));
                        h.battery_icon.set_visible(true);
                    }
                    None => {
                        h.battery.set_subtitle("");
                        h.battery_value.set_label(&gettext("Unknown"));
                        h.battery_icon.set_visible(false);
                    }
                }
                let unknown = gettext("Unknown");
                h.dpi.set_label(s.dpi.as_deref().unwrap_or(&unknown));
                h.wheel.set_label(s.wheel.as_deref().unwrap_or(&unknown));
                h.states.set_visible_child_name("connected");
            }
        }
    }
}
