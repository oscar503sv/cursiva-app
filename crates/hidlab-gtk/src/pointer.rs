//! The "Pointer and scrolling" page (mockup 2): pointer speed and GNOME
//! acceleration, the main wheel's mode and push, GNOME natural scrolling,
//! the thumb wheel direction, and exact values.
//!
//! The page holds no device logic: each control sends a [`Request`] and
//! the window turns it into daemon calls (`crate::changes`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use hidlab_api::daemon::Status;
use hidlab_api::settings::{SettingKind, SettingValue};

use crate::changes::{self, ACCELERATION, DpiScale, WheelMode};
use crate::model::{View, fill};

/// Wait after the last movement of a slider or number before sending.
const SETTLE: Duration = Duration::from_millis(300);

/// What the user asked for on this page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Dpi(u16),
    Threshold(u8),
    WheelMode(WheelMode),
    Invert(bool),
    NaturalScroll(bool),
    Acceleration(&'static str),
    /// A section's "Restore Default".
    Reset(Section),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Pointer,
    Wheel,
    Thumb,
}

impl Section {
    /// Mouse settings the section's "Restore Default" resets. GNOME
    /// settings are not reset: they are shared with every mouse.
    pub fn kinds(self) -> &'static [SettingKind] {
        match self {
            Section::Pointer => &[SettingKind::Dpi],
            Section::Wheel => &[
                SettingKind::SmartshiftMode,
                SettingKind::SmartshiftThreshold,
            ],
            Section::Thumb => &[SettingKind::ThumbwheelInvert],
        }
    }

    pub fn reset_label(self) -> String {
        match self {
            Section::Pointer => gettext("Pointer speed restored"),
            Section::Wheel => gettext("Main wheel restored"),
            Section::Thumb => gettext("Thumb wheel restored"),
        }
    }
}

type Sink = Rc<dyn Fn(Request)>;

pub struct PointerPage {
    pub root: gtk::Stack,
    unavailable: adw::StatusPage,
    scroller: gtk::ScrolledWindow,
    column: gtk::Box,
    pointer: adw::PreferencesGroup,
    wheel: adw::PreferencesGroup,
    thumb: adw::PreferencesGroup,
    dpi_value: gtk::Label,
    dpi: gtk::Scale,
    accel: adw::ComboRow,
    mode: adw::ToggleGroup,
    mode_text: gtk::Label,
    push_row: adw::PreferencesRow,
    push: gtk::Scale,
    natural: adw::SwitchRow,
    invert: adw::SwitchRow,
    exact_dpi: adw::SpinRow,
    exact_threshold: adw::SpinRow,
    /// Set while the page shows a new status, so its own updates send
    /// nothing.
    syncing: Cell<bool>,
    dpi_scale: RefCell<Option<DpiScale>>,
    /// Debounce timers by control, see [`SETTLE`].
    pending: RefCell<HashMap<&'static str, glib::SourceId>>,
    /// True while a mouse button or finger holds a slider.
    dragging: Cell<bool>,
    /// Requests kept until the slider is let go, by control.
    held: RefCell<HashMap<&'static str, (Sink, Request)>>,
}

fn dim(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes(["dim-label", "caption"])
        .build()
}

/// A row with a title, a value on the right, a slider and the two ends'
/// names under it.
fn slider_row(
    title: &str,
    scale: &gtk::Scale,
    low: &str,
    high: &str,
) -> (adw::PreferencesRow, gtk::Label) {
    let value = gtk::Label::builder()
        .css_classes(["dim-label", "numeric"])
        .halign(gtk::Align::End)
        .hexpand(true)
        .build();
    let head = gtk::Box::builder().spacing(12).build();
    head.append(&gtk::Label::builder().label(title).xalign(0.0).build());
    head.append(&value);
    let ends = gtk::Box::builder().build();
    let low = dim(low);
    low.set_hexpand(true);
    low.set_xalign(0.0);
    let high = dim(high);
    high.set_xalign(1.0);
    ends.append(&low);
    ends.append(&high);
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    column.append(&head);
    column.append(scale);
    column.append(&ends);
    scale.update_property(&[gtk::accessible::Property::Label(title)]);
    let row = adw::PreferencesRow::builder()
        .title(title)
        .activatable(false)
        .child(&column)
        .build();
    (row, value)
}

/// The pointer speed slider. Logarithmic: its positions are ln(DPI) (see
/// `DpiScale::position`), so it must not round them; rounding to whole
/// numbers left only about 200, 400, 1100, 3000 and 8000 DPI.
fn dpi_slider() -> gtk::Scale {
    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        DpiScale::position(200),
        DpiScale::position(8000),
        0.05,
    );
    scale.set_draw_value(false);
    scale.set_round_digits(-1);
    scale
}

fn restore_button(section: Section, sink: &Sink) -> gtk::Button {
    let button = gtk::Button::builder()
        .label(gettext("Restore Default"))
        .css_classes(["flat"])
        .valign(gtk::Align::Center)
        .build();
    let sink = sink.clone();
    button.connect_clicked(move |_| sink(Request::Reset(section)));
    button
}

impl PointerPage {
    pub fn new(sink: Sink) -> Rc<PointerPage> {
        let root = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        let unavailable = adw::StatusPage::builder()
            .icon_name("input-mouse-symbolic")
            .build();

        // Pointer.
        let dpi = dpi_slider();
        let (dpi_row, dpi_value) = slider_row(
            &gettext("Speed"),
            &dpi,
            &gettext("Slower"),
            &gettext("Faster"),
        );
        let accel_names = gtk::StringList::new(&[]);
        for v in ACCELERATION {
            accel_names.append(&changes::acceleration_label(v));
        }
        let accel = adw::ComboRow::builder()
            .title(gettext("Acceleration"))
            .subtitle(gettext("GNOME setting, for every mouse"))
            .model(&accel_names)
            .build();
        let pointer = adw::PreferencesGroup::builder()
            .title(gettext("Pointer"))
            .build();
        pointer.add(&dpi_row);
        pointer.add(&accel);

        // Main wheel.
        let mode = adw::ToggleGroup::builder().hexpand(true).build();
        for m in WheelMode::ALL {
            mode.add(adw::Toggle::builder().name(m.id()).label(m.label()).build());
        }
        let mode_text = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .xalign(0.0)
            .build();
        let mode_column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        let mode_title = gettext("Scroll mode");
        mode_column.append(&gtk::Label::builder().label(&mode_title).xalign(0.0).build());
        mode_column.append(&mode);
        mode_column.append(&mode_text);
        mode.upcast_ref::<gtk::Widget>()
            .update_property(&[gtk::accessible::Property::Label(&mode_title)]);
        let mode_row = adw::PreferencesRow::builder()
            .title(&mode_title)
            .activatable(false)
            .child(&mode_column)
            .build();
        let push = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 50.0, 1.0);
        push.set_draw_value(false);
        push.set_round_digits(0);
        let (push_row, _) = slider_row(
            &gettext("Switches to free spin with"),
            &push,
            &gettext("A light push"),
            &gettext("A hard push"),
        );
        let natural = adw::SwitchRow::builder()
            .title(gettext("Natural scrolling"))
            .subtitle(gettext(
                "Content follows your finger. GNOME setting, for every mouse",
            ))
            .build();
        let smooth = adw::ActionRow::builder()
            .title(gettext("Smooth scrolling"))
            .subtitle(gettext("High-resolution wheel, managed by the system"))
            .build();
        smooth.add_suffix(&dim(&gettext("On")));
        let wheel = adw::PreferencesGroup::builder()
            .title(gettext("Main wheel"))
            .build();
        wheel.add(&mode_row);
        wheel.add(&push_row);
        wheel.add(&natural);
        wheel.add(&smooth);

        // Thumb wheel.
        let invert = adw::SwitchRow::builder()
            .title(gettext("Reverse direction"))
            .subtitle(gettext("Scrolls sideways the other way"))
            .build();
        let thumb = adw::PreferencesGroup::builder()
            .title(gettext("Thumb wheel"))
            .build();
        thumb.add(&invert);

        // Advanced.
        let exact_dpi = adw::SpinRow::with_range(200.0, 8000.0, 50.0);
        exact_dpi.set_title(&gettext("Resolution"));
        let exact_threshold = adw::SpinRow::with_range(1.0, 50.0, 1.0);
        exact_threshold.set_title(&gettext("SmartShift threshold"));
        exact_threshold.set_subtitle(&gettext("1 (light) to 50 (hard)"));
        let exact = adw::ExpanderRow::builder()
            .title(gettext("Exact values"))
            .build();
        exact.add_row(&exact_dpi);
        exact.add_row(&exact_threshold);
        let advanced = adw::PreferencesGroup::builder()
            .title(gettext("Advanced"))
            .build();
        advanced.add(&exact);

        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(24)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(12)
            .margin_end(12)
            .build();
        for g in [&pointer, &wheel, &thumb, &advanced] {
            column.append(g);
        }
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(
                &adw::Clamp::builder()
                    .maximum_size(640)
                    .child(&column)
                    .build(),
            )
            .build();
        root.add_named(&scroller, Some("content"));
        root.add_named(&unavailable, Some("unavailable"));

        pointer.set_header_suffix(Some(&restore_button(Section::Pointer, &sink)));
        wheel.set_header_suffix(Some(&restore_button(Section::Wheel, &sink)));
        thumb.set_header_suffix(Some(&restore_button(Section::Thumb, &sink)));

        let page = Rc::new(PointerPage {
            root,
            unavailable,
            scroller,
            column,
            pointer,
            wheel,
            thumb,
            dpi_value,
            dpi,
            accel,
            mode,
            mode_text,
            push_row,
            push,
            natural,
            invert,
            exact_dpi,
            exact_threshold,
            syncing: Cell::new(false),
            dpi_scale: RefCell::new(None),
            pending: RefCell::new(HashMap::new()),
            dragging: Cell::new(false),
            held: RefCell::new(HashMap::new()),
        });
        page.connect(&sink);
        page.hold_while_pressed(&page.dpi);
        page.hold_while_pressed(&page.push);
        page
    }

    fn connect(self: &Rc<Self>, sink: &Sink) {
        let weak = Rc::downgrade(self);
        let on = move |f: &dyn Fn(&Rc<PointerPage>)| {
            if let Some(p) = weak.upgrade()
                && !p.syncing.get()
            {
                f(&p);
            }
        };

        let (o, s) = (on.clone(), sink.clone());
        self.dpi.connect_value_changed(move |scale| {
            o(&|p| {
                let Some(d) = p.dpi_scale.borrow().as_ref().map(|d| d.at(scale.value())) else {
                    return;
                };
                p.show_dpi(d);
                p.later("dpi", s.clone(), Request::Dpi(d));
            });
        });
        let (o, s) = (on.clone(), sink.clone());
        self.exact_dpi.connect_value_notify(move |row| {
            o(&|p| {
                let Some(d) = p.dpi_scale.borrow().as_ref().map(|d| d.snap(row.value())) else {
                    return;
                };
                p.later("dpi", s.clone(), Request::Dpi(d));
            });
        });
        let (o, s) = (on.clone(), sink.clone());
        self.push.connect_value_changed(move |scale| {
            o(&|p| {
                p.later(
                    "threshold",
                    s.clone(),
                    Request::Threshold(scale.value() as u8),
                )
            });
        });
        let (o, s) = (on.clone(), sink.clone());
        self.exact_threshold.connect_value_notify(move |row| {
            o(&|p| {
                p.later(
                    "threshold",
                    s.clone(),
                    Request::Threshold(row.value() as u8),
                )
            });
        });
        let (o, s) = (on.clone(), sink.clone());
        self.mode.connect_active_name_notify(move |group| {
            o(&|p| {
                let Some(m) = group.active_name().and_then(|n| WheelMode::from_id(&n)) else {
                    return;
                };
                p.show_mode(m);
                s(Request::WheelMode(m));
            });
        });
        let (o, s) = (on.clone(), sink.clone());
        self.natural.connect_active_notify(move |row| {
            o(&|_| s(Request::NaturalScroll(row.is_active())));
        });
        let (o, s) = (on.clone(), sink.clone());
        self.invert.connect_active_notify(move |row| {
            o(&|_| s(Request::Invert(row.is_active())));
        });
        let (o, s) = (on, sink.clone());
        self.accel.connect_selected_notify(move |row| {
            o(&|_| {
                if let Some(v) = ACCELERATION.get(row.selected() as usize) {
                    s(Request::Acceleration(v));
                }
            });
        });
    }

    /// Sends `request` when the slider is let go or, for keys and the
    /// scroll wheel, once the control has been still for [`SETTLE`].
    fn later(self: &Rc<Self>, key: &'static str, sink: Sink, request: Request) {
        if let Some(id) = self.pending.borrow_mut().remove(key) {
            id.remove();
        }
        if self.dragging.get() {
            self.held.borrow_mut().insert(key, (sink, request));
            return;
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(SETTLE, move || {
            if let Some(p) = weak.upgrade() {
                p.pending.borrow_mut().remove(key);
            }
            sink(request);
        });
        self.pending.borrow_mut().insert(key, id);
    }

    /// Watches presses on a slider, so a drag sends one change at the end
    /// rather than one per pause.
    fn hold_while_pressed(self: &Rc<Self>, scale: &gtk::Scale) {
        let events = gtk::EventControllerLegacy::new();
        events.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        events.connect_event(move |_, event| {
            let Some(p) = weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            match event.event_type() {
                gtk::gdk::EventType::ButtonPress | gtk::gdk::EventType::TouchBegin => {
                    p.dragging.set(true);
                }
                gtk::gdk::EventType::ButtonRelease
                | gtk::gdk::EventType::TouchEnd
                | gtk::gdk::EventType::TouchCancel => {
                    p.dragging.set(false);
                    let held: Vec<_> = p.held.borrow_mut().drain().collect();
                    for (_, (sink, request)) in held {
                        sink(request);
                    }
                }
                _ => {}
            }
            glib::Propagation::Proceed
        });
        scale.add_controller(events);
    }

    fn busy(&self, key: &'static str) -> bool {
        self.pending.borrow().contains_key(key) || self.held.borrow().contains_key(key)
    }

    fn show_dpi(&self, d: u16) {
        self.dpi_value
            .set_label(&fill(&gettext("{dpi} DPI"), &[("dpi", &d.to_string())]));
    }

    fn show_mode(&self, m: WheelMode) {
        self.mode_text.set_label(&m.description());
        self.push_row.set_visible(m == WheelMode::Auto);
        self.exact_threshold.set_sensitive(m == WheelMode::Auto);
    }

    /// Shows the daemon's state; the controls follow the mouse.
    pub fn update(&self, view: &View, status: Option<&Status>) {
        let status = match (view, status) {
            (View::Connected(_), Some(s)) => s,
            _ => {
                let (title, text) = match view {
                    View::Connecting => {
                        (gettext("Connecting to the Cursiva service…"), String::new())
                    }
                    View::NoDaemon(e) => (gettext("Cannot reach the Cursiva service"), e.clone()),
                    _ => (
                        gettext("Connect your mouse"),
                        gettext(
                            "Turn the mouse on and connect it over Bluetooth. Cursiva finds it by itself.",
                        ),
                    ),
                };
                self.unavailable.set_title(&title);
                self.unavailable
                    .set_description(Some(&glib::markup_escape_text(&text)));
                self.root.set_visible_child_name("unavailable");
                return;
            }
        };
        self.syncing.set(true);
        let scale = changes::dpi_scale(status);
        if let Some(s) = &scale {
            self.dpi.clear_marks();
            self.dpi
                .set_range(DpiScale::position(s.min), DpiScale::position(s.max));
            self.dpi.set_increments(0.05, 0.25);
            for &(v, default) in &s.marks {
                let (label, side) = if default {
                    let text = fill(&gettext("{dpi} · default"), &[("dpi", &v.to_string())]);
                    (text, gtk::PositionType::Top)
                } else {
                    (v.to_string(), gtk::PositionType::Bottom)
                };
                self.dpi.add_mark(DpiScale::position(v), side, Some(&label));
            }
            self.exact_dpi.set_range(f64::from(s.min), f64::from(s.max));
            self.exact_dpi
                .adjustment()
                .set_step_increment(f64::from(s.step));
            self.exact_dpi.set_subtitle(&fill(
                // TRANSLATORS: the DPI range the mouse accepts.
                &gettext("From {min} to {max} in steps of {step}"),
                &[
                    ("min", &s.min.to_string()),
                    ("max", &s.max.to_string()),
                    ("step", &s.step.to_string()),
                ],
            ));
        }
        self.dpi.set_sensitive(scale.is_some());
        self.exact_dpi.set_sensitive(scale.is_some());
        *self.dpi_scale.borrow_mut() = scale;

        if !self.busy("dpi")
            && let Some(SettingValue::Dpi(d)) = changes::current(status, SettingKind::Dpi)
        {
            self.dpi.set_value(DpiScale::position(d));
            self.exact_dpi.set_value(f64::from(d));
            self.show_dpi(d);
        }
        if let Some(m) = changes::wheel_mode(status) {
            self.mode.set_active_name(Some(m.id()));
            self.show_mode(m);
        }
        self.mode
            .set_sensitive(changes::wheel_mode(status).is_some());
        if !self.busy("threshold") {
            let t = f64::from(changes::push_threshold(status));
            self.push.set_value(t);
            self.exact_threshold.set_value(t);
        }
        if let Some(SettingValue::ThumbwheelInvert(i)) =
            changes::current(status, SettingKind::ThumbwheelInvert)
        {
            self.invert.set_active(i);
        }
        let desktop = &status.desktop;
        self.natural.set_sensitive(desktop.natural_scroll.is_some());
        self.natural
            .set_active(desktop.natural_scroll.unwrap_or(false));
        let accel = desktop
            .acceleration
            .as_deref()
            .and_then(|a| ACCELERATION.iter().position(|v| *v == a));
        self.accel.set_sensitive(accel.is_some());
        if let Some(i) = accel {
            self.accel.set_selected(i as u32);
        }
        self.syncing.set(false);
        self.root.set_visible_child_name("content");
    }

    /// Scrolls so `section` is at the top.
    pub fn scroll_to(&self, section: Section) {
        let group = match section {
            Section::Pointer => &self.pointer,
            Section::Wheel => &self.wheel,
            Section::Thumb => &self.thumb,
        };
        let (group, column, scroller) = (group.clone(), self.column.clone(), self.scroller.clone());
        glib::idle_add_local_once(move || {
            if let Some(p) = group.compute_point(&column, &gtk::graphene::Point::zero()) {
                scroller.vadjustment().set_value(f64::from(p.y()));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a display to create the widget; skipped without one.
    #[test]
    fn dpi_slider_reaches_every_step() {
        if gtk::init().is_err() {
            eprintln!("no display: slider test skipped");
            return;
        }
        let slider = dpi_slider();
        let scale = DpiScale {
            min: 200,
            max: 8000,
            step: 50,
            marks: vec![],
        };
        for dpi in [200, 650, 1000, 1600, 2400, 5050, 8000] {
            // As a drag does: `change-value` applies `round-digits`;
            // `set_value` would not.
            let _: bool = slider.emit_by_name(
                "change-value",
                &[&gtk::ScrollType::Jump, &DpiScale::position(dpi)],
            );
            assert_eq!(scale.at(slider.value()), dpi);
        }
    }
}
