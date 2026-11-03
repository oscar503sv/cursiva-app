//! The main window: a sidebar with Home, Pointer and scrolling, and
//! Buttons. Pages follow the daemon live; changes go to the daemon one at
//! a time and show a toast with Undo.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use hidlab_api::daemon::{DesktopSetting, Status, UNIT};
use hidlab_api::settings::SettingValue;

use crate::buttons::ButtonsPage;
use crate::changes::{self, Change, Op};
use crate::dbus::{self, Event, Subscriptions};
use crate::geometry::Hotspot;
use crate::model::{Health, View, fill};
use crate::mouse::MouseView;
use crate::pointer::PointerPage;
use crate::remaps;
use crate::request::{Request, Section};

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
    mouse: MouseView,
    dpi_row: adw::ActionRow,
    wheel_row: adw::ActionRow,
}

/// A success toast on screen, the settings its change touched
/// (`changes::targets`) and its Undo.
type ShownToast = (adw::Toast, Vec<String>, Vec<Op>);

/// Widgets that switch pages.
struct Nav {
    list: gtk::ListBox,
    stack: gtk::Stack,
    content: adw::NavigationPage,
    split: adw::NavigationSplitView,
}

struct App {
    home: Home,
    pointer: Rc<PointerPage>,
    nav: Nav,
    toasts: adw::ToastOverlay,
    /// Changes waiting for the daemon, run one at a time; the flag says
    /// whether to offer Undo.
    queue: RefCell<VecDeque<(Change, bool)>>,
    running: Cell<bool>,
    /// The success toast on screen: the settings it changed and its Undo.
    last_toast: RefCell<Option<ShownToast>>,
    conn: RefCell<Option<gio::DBusConnection>>,
    subscriptions: RefCell<Option<Subscriptions>>,
    status: RefCell<Option<Status>>,
    fetching: Cell<bool>,
    fetch_again: Cell<bool>,
    /// The daemon answered at least once; a lost name owner then means it
    /// stopped, rather than not started yet.
    seen_daemon: Cell<bool>,
    buttons: Rc<ButtonsPage>,
    /// Renews the `WatchButtons` lease while the Buttons page is shown.
    lease: RefCell<Option<glib::SourceId>>,
}

/// `WatchButtons` lease length and renewal period.
const LEASE_S: u32 = 30;
const RENEW: std::time::Duration = std::time::Duration::from_secs(20);

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

    let state = Rc::new_cyclic(|weak: &Weak<App>| {
        let sink: crate::request::Sink = {
            let weak = weak.clone();
            Rc::new(move |r| {
                if let Some(a) = weak.upgrade() {
                    a.request(r);
                }
            })
        };
        let pointer = PointerPage::new(sink.clone());
        let buttons = ButtonsPage::new(sink);
        let stack = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        let (home_widget, home) = home_page();
        let pages = pages();
        stack.add_titled(&home_widget, Some(pages[0].name), &pages[0].title);
        stack.add_titled(&pointer.root, Some(pages[1].name), &pages[1].title);
        stack.add_titled(&buttons.root, Some(pages[2].name), &pages[2].title);
        let content = adw::NavigationPage::builder()
            .title(&pages[0].title)
            .tag("content")
            .child(&with_header(&stack, false))
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
        list.select_row(list.row_at_index(0).as_ref());
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
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&split));
        App {
            home,
            pointer,
            nav: Nav {
                list,
                stack,
                content,
                split,
            },
            toasts,
            queue: RefCell::new(VecDeque::new()),
            running: Cell::new(false),
            last_toast: RefCell::new(None),
            conn: RefCell::new(None),
            subscriptions: RefCell::new(None),
            status: RefCell::new(None),
            fetching: Cell::new(false),
            fetch_again: Cell::new(false),
            seen_daemon: Cell::new(false),
            buttons,
            lease: RefCell::new(None),
        }
    });

    let weak = Rc::downgrade(&state);
    state.nav.list.connect_row_activated(move |_, row| {
        if let Some(s) = weak.upgrade() {
            s.show_page(&row.widget_name(), None);
        }
    });
    let weak = Rc::downgrade(&state);
    state.home.mouse.connect_hotspot_activated(move |_, id| {
        if let Some(s) = weak.upgrade() {
            s.activate_part(id, false);
        }
    });
    let weak = Rc::downgrade(&state);
    state.buttons.mouse.connect_hotspot_activated(move |_, id| {
        if let Some(s) = weak.upgrade() {
            s.activate_part(id, true);
        }
    });
    for (row, section) in [
        (&state.home.dpi_row, Section::Pointer),
        (&state.home.wheel_row, Section::Wheel),
    ] {
        let weak = Rc::downgrade(&state);
        row.connect_activated(move |_| {
            if let Some(s) = weak.upgrade() {
                s.show_page("pointer", Some(section));
            }
        });
    }

    let narrow =
        adw::Breakpoint::new(adw::BreakpointCondition::parse(NARROW).expect("valid condition"));
    narrow.add_setter(&state.nav.split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(narrow);
    window.set_content(Some(&state.toasts));

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
    // GNOME settings changed elsewhere send no signal: read again when the
    // window comes back to the front.
    let weak = Rc::downgrade(&state);
    window.connect_is_active_notify(move |w| {
        if let Some(s) = weak.upgrade() {
            if w.is_active() {
                s.refresh();
            }
            s.update_lease();
        }
    });
    glib::spawn_future_local(start(Rc::downgrade(&state)));
    #[cfg(debug_assertions)]
    dev_screenshot(&window, &state);
    // The window keeps the state alive; the async parts hold weak references.
    window.connect_destroy(move |_| {
        let _ = &state;
    });
    window
}

/// Development aid, debug builds only: with `CURSIVA_DEV_SCREENSHOT=<png>`
/// the app opens `CURSIVA_DEV_PAGE` (`home`, `pointer`, `pointer:wheel`,
/// `pointer:thumb`, `buttons`), waits `CURSIVA_DEV_DELAY_MS` (default
/// 3000), saves the window as a PNG and quits. `CURSIVA_DEV_SIZE=<w>x<h>`
/// sets the window size. Used with the broadway backend to look at pages
/// without a desktop session; see CONTRIBUTING.md.
#[cfg(debug_assertions)]
fn dev_screenshot(window: &adw::ApplicationWindow, state: &Rc<App>) {
    let Some(path) = std::env::var_os("CURSIVA_DEV_SCREENSHOT") else {
        return;
    };
    let var = |name: &str| std::env::var(name).ok();
    if let Some((w, h)) = var("CURSIVA_DEV_SIZE").and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        window.set_default_size(w, h);
    }
    let page = var("CURSIVA_DEV_PAGE").unwrap_or_else(|| "home".into());
    let delay = var("CURSIVA_DEV_DELAY_MS")
        .and_then(|d| d.parse().ok())
        .unwrap_or(3000);
    let (window, weak) = (window.clone(), Rc::downgrade(state));
    glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
        if let Some(s) = weak.upgrade() {
            let (name, section) = page.split_once(':').unwrap_or((&page, ""));
            let section_of = match section {
                "wheel" => Some(Section::Wheel),
                "thumb" => Some(Section::Thumb),
                _ => None,
            };
            s.show_page(name, section_of);
            // `buttons:back` and so on open that button's picker.
            if let Some(c) = remaps::ROWS.into_iter().find(|c| c.name() == section) {
                s.buttons.open_picker(c);
            }
        }
        glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
            let (w, h) = (window.width(), window.height());
            let paintable = gtk::WidgetPaintable::new(Some(&window));
            let snapshot = gtk::Snapshot::new();
            paintable.snapshot(&snapshot, f64::from(w), f64::from(h));
            let saved = snapshot
                .to_node()
                .zip(window.renderer())
                .map(|(node, renderer)| {
                    let rect = gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32);
                    renderer
                        .render_texture(node, Some(&rect))
                        .save_to_png(&path)
                });
            match saved {
                Some(Ok(())) => eprintln!("screenshot saved to {}", path.to_string_lossy()),
                Some(Err(e)) => eprintln!("cannot save the screenshot: {e}"),
                None => eprintln!("nothing to save: the window has no content or renderer"),
            }
            if let Some(app) = window.application() {
                app.quit();
            }
        });
    });
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
    for r in [&dpi_row, &wheel_row] {
        r.set_activatable(true);
        r.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    }
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
        mouse,
        dpi_row,
        wheel_row,
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
    fn show_page(self: &Rc<Self>, name: &str, section: Option<Section>) {
        let nav = &self.nav;
        let index = pages().iter().position(|p| p.name == name);
        if let Some(row) = index.and_then(|i| nav.list.row_at_index(i as i32)) {
            nav.list.select_row(Some(&row));
        }
        nav.stack.set_visible_child_name(name);
        if let Some(child) = nav.stack.visible_child()
            && let Some(title) = nav.stack.page(&child).title()
        {
            nav.content.set_title(&title);
        }
        nav.split.set_show_content(true);
        if let Some(section) = section {
            self.pointer.scroll_to(section);
        }
        self.update_lease();
    }

    /// A part of a drawing was clicked. On Home the wheel opens its
    /// scrolling settings; on the Buttons page it is the middle button.
    fn activate_part(self: &Rc<Self>, id: &str, on_buttons: bool) {
        let Some(spot) = Hotspot::ALL.into_iter().find(|h| h.id() == id) else {
            return;
        };
        match (spot, remaps::control(spot)) {
            (Hotspot::Wheel, _) if !on_buttons => self.show_page("pointer", Some(Section::Wheel)),
            (Hotspot::Thumb, _) => self.show_page("pointer", Some(Section::Thumb)),
            (_, Some(control)) => {
                self.show_page("buttons", None);
                self.buttons.open_picker(control);
            }
            _ => {
                self.show_page("buttons", None);
                self.toasts
                    .add_toast(adw::Toast::new(&glib::markup_escape_text(&gettext(
                        "Left and right click are not reassigned, so the mouse always works.",
                    ))));
            }
        }
    }

    /// Asks for button presses while the Buttons page is on screen with
    /// the mouse connected, and stops asking otherwise.
    fn update_lease(self: &Rc<Self>) {
        let active = self
            .nav
            .stack
            .root()
            .and_downcast::<gtk::Window>()
            .is_some_and(|w| w.is_active());
        let want = active
            && self.nav.stack.visible_child_name().as_deref() == Some("buttons")
            && self
                .status
                .borrow()
                .as_ref()
                .is_some_and(|s| s.device.is_some());
        let has = self.lease.borrow().is_some();
        if want == has {
            return;
        }
        let Some(conn) = self.conn.borrow().clone() else {
            return;
        };
        if want {
            let renew = move |conn: gio::DBusConnection| {
                glib::spawn_future_local(async move {
                    if let Err(e) = dbus::watch_buttons(&conn, LEASE_S).await {
                        eprintln!("cursiva: cannot watch button presses: {e}");
                    }
                });
            };
            renew(conn.clone());
            let id = glib::timeout_add_local(RENEW, move || {
                renew(conn.clone());
                glib::ControlFlow::Continue
            });
            *self.lease.borrow_mut() = Some(id);
        } else {
            if let Some(id) = self.lease.borrow_mut().take() {
                id.remove();
            }
            self.buttons.clear_presses();
            glib::spawn_future_local(async move {
                let _ = dbus::watch_buttons(&conn, 0).await;
            });
        }
    }

    /// Turns a page's request into daemon calls, from the last status.
    fn request(self: &Rc<Self>, request: Request) {
        let change = {
            let status = self.status.borrow();
            let Some(st) = status.as_ref() else { return };
            let on_off = |b: bool| if b { "on" } else { "off" };
            match request {
                Request::Dpi(d) => changes::set(st, SettingValue::Dpi(d)),
                Request::Threshold(t) => changes::set(st, SettingValue::SmartshiftThreshold(t)),
                Request::WheelMode(m) => changes::set_wheel_mode(st, m),
                Request::Invert(i) => changes::set(st, SettingValue::ThumbwheelInvert(i)),
                Request::NaturalScroll(n) => {
                    changes::set_desktop(st, DesktopSetting::NaturalScroll, on_off(n))
                }
                Request::Acceleration(a) => {
                    changes::set_desktop(st, DesktopSetting::Acceleration, a)
                }
                Request::Remap(c, t) => changes::set(st, SettingValue::ButtonRemap(c, t)),
                Request::Reset(section) => {
                    changes::reset(st, section.kinds(), section.reset_label())
                }
            }
        };
        match change {
            Some(c) => self.execute(c, true),
            // Nothing to send: show the status again in case a control
            // moved away from it.
            None => self.refresh(),
        }
    }

    /// Queues a change; the queue runs one daemon call at a time.
    fn execute(self: &Rc<Self>, change: Change, undoable: bool) {
        self.queue.borrow_mut().push_back((change, undoable));
        if self.running.replace(true) {
            return;
        }
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            loop {
                let Some(s) = weak.upgrade() else { return };
                let next = s.queue.borrow_mut().pop_front();
                let Some((change, undoable)) = next else {
                    s.running.set(false);
                    return;
                };
                let Some(conn) = s.conn.borrow().clone() else {
                    s.queue.borrow_mut().clear();
                    s.running.set(false);
                    return;
                };
                drop(s);
                let mut error = None;
                for op in &change.ops {
                    if let Err(e) = dbus::run(&conn, op).await {
                        error = Some(e);
                        break;
                    }
                }
                let Some(s) = weak.upgrade() else { return };
                s.toast(&change, undoable, error);
                s.refresh();
            }
        });
    }

    fn toast(self: &Rc<Self>, change: &Change, undoable: bool, error: Option<String>) {
        let toast = match error {
            Some(e) => {
                let t = adw::Toast::new(&glib::markup_escape_text(&fill(
                    // TRANSLATORS: {error} is the daemon's message, in English.
                    &gettext("Not changed: {error}"),
                    &[("error", &e)],
                )));
                t.set_timeout(10);
                t
            }
            None if undoable => {
                // One toast at a time: a quick follow-up change of the same
                // setting replaces it and keeps its Undo (see
                // `changes::merged_undo`).
                let previous = self.last_toast.borrow_mut().take();
                let undo_ops = changes::merged_undo(
                    previous
                        .as_ref()
                        .map(|(_, t, u)| (t.as_slice(), u.as_slice())),
                    change,
                );
                if let Some((toast, _, _)) = previous {
                    toast.dismiss();
                }
                let t = adw::Toast::new(&glib::markup_escape_text(&change.label));
                *self.last_toast.borrow_mut() =
                    Some((t.clone(), changes::targets(&change.ops), undo_ops.clone()));
                let weak = Rc::downgrade(self);
                t.connect_dismissed(move |t| {
                    if let Some(s) = weak.upgrade() {
                        let mut last = s.last_toast.borrow_mut();
                        if last.as_ref().is_some_and(|(shown, _, _)| shown == t) {
                            *last = None;
                        }
                    }
                });
                if !undo_ops.is_empty() {
                    t.set_button_label(Some(&gettext("Undo")));
                    let undo = Change {
                        ops: undo_ops,
                        undo: Vec::new(),
                        label: String::new(),
                    };
                    let weak = Rc::downgrade(self);
                    t.connect_button_clicked(move |_| {
                        if let Some(s) = weak.upgrade() {
                            s.execute(undo.clone(), false);
                        }
                    });
                }
                t
            }
            None => return,
        };
        self.toasts.add_toast(toast);
    }

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
            Event::Button(e) => {
                let parts = match (&e.control, self.status.borrow().as_ref()) {
                    (Some(task), Some(st)) => remaps::pressed_parts(st, task),
                    _ => return,
                };
                self.buttons.show_press(&parts, e.pressed);
                // The mode button reports its press only.
                if e.pressed && e.number.is_none() {
                    let weak = Rc::downgrade(self);
                    glib::timeout_add_local_once(
                        std::time::Duration::from_millis(300),
                        move || {
                            if let Some(s) = weak.upgrade() {
                                s.buttons.show_press(&parts, false);
                            }
                        },
                    );
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

    fn render(self: &Rc<Self>, view: &View) {
        self.pointer.update(view, self.status.borrow().as_ref());
        self.buttons.update(view, self.status.borrow().as_ref());
        self.update_lease();
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
