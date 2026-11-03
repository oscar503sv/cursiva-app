//! The Buttons page (mockup 3) and its action picker (mockup 4): what each
//! remappable button does, live presses, and choosing another task.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use hidlab_api::daemon::Status;
use hidlab_api::remap::RemapControl;

use crate::geometry::Hotspot;
use crate::model::View;
use crate::mouse::MouseView;
use crate::remaps::{self, ROWS};
use crate::request::{Request, Section, Sink};

/// CSS class of a row whose button is held down.
const PRESSED: &str = "cursiva-pressed";

struct Row {
    control: RemapControl,
    row: adw::ActionRow,
    value: gtk::Label,
}

pub struct ButtonsPage {
    pub root: gtk::Stack,
    pub mouse: MouseView,
    unavailable: adw::StatusPage,
    rows: Vec<Row>,
    sink: Sink,
    status: RefCell<Option<Status>>,
}

impl ButtonsPage {
    pub fn new(sink: Sink) -> Rc<ButtonsPage> {
        let root = gtk::Stack::builder()
            .transition_type(gtk::StackTransitionType::Crossfade)
            .build();
        let unavailable = adw::StatusPage::builder()
            .icon_name("input-mouse-symbolic")
            .build();

        let mouse = MouseView::new();
        mouse.set_halign(gtk::Align::Center);
        // The thumb wheel is not a button; it is set on the Pointer page.
        mouse.set_disabled(Hotspot::Thumb, true);
        let hint = gtk::Label::builder()
            .label(gettext("Press a button on the mouse to find it here"))
            .css_classes(["dim-label"])
            .wrap(true)
            .justify(gtk::Justification::Center)
            .build();

        let group = adw::PreferencesGroup::builder()
            .title(gettext("What each button does"))
            .build();
        let restore = gtk::Button::builder()
            .label(gettext("Restore All"))
            .css_classes(["flat"])
            .valign(gtk::Align::Center)
            .build();
        let s = sink.clone();
        restore.connect_clicked(move |_| s(Request::Reset(Section::Buttons)));
        group.set_header_suffix(Some(&restore));
        let rows: Vec<Row> = ROWS
            .iter()
            .map(|&control| {
                let row = adw::ActionRow::builder()
                    .title(remaps::button_name(control))
                    .subtitle(remaps::button_place(control))
                    .activatable(true)
                    .build();
                let value = gtk::Label::builder()
                    .css_classes(["dim-label"])
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build();
                row.add_suffix(&value);
                row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
                group.add(&row);
                Row {
                    control,
                    row,
                    value,
                }
            })
            .collect();
        let note = gtk::Label::builder()
            .label(gettext(
                "Left and right click are not reassigned, so the mouse always works.",
            ))
            .css_classes(["dim-label", "caption"])
            .wrap(true)
            .xalign(0.0)
            .build();

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
        column.append(&group);
        column.append(&note);
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

        let page = Rc::new(ButtonsPage {
            root,
            mouse,
            unavailable,
            rows,
            sink,
            status: RefCell::new(None),
        });
        for r in &page.rows {
            let (weak, control) = (Rc::downgrade(&page), r.control);
            r.row.connect_activated(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.open_picker(control);
                }
            });
        }
        page
    }

    pub fn update(&self, view: &View, status: Option<&Status>) {
        let Some(status) = status.filter(|_| matches!(view, View::Connected(_))) else {
            let (title, text) = view.unavailable();
            self.unavailable.set_title(&title);
            self.unavailable
                .set_description(Some(&glib::markup_escape_text(&text)));
            self.root.set_visible_child_name("unavailable");
            self.status.borrow_mut().take();
            return;
        };
        for r in &self.rows {
            let text = remaps::current_target(status, r.control)
                .map(remaps::action_name)
                .unwrap_or_else(|| gettext("Unknown"));
            r.value.set_label(&text);
        }
        *self.status.borrow_mut() = Some(status.clone());
        self.root.set_visible_child_name("content");
    }

    /// Lights or clears parts and their rows for a press.
    pub fn show_press(&self, parts: &[Hotspot], pressed: bool) {
        for &h in parts {
            self.mouse.set_pressed(h, pressed);
        }
        for r in &self.rows {
            if parts.contains(&remaps::hotspot(r.control)) {
                if pressed {
                    r.row.add_css_class(PRESSED);
                } else {
                    r.row.remove_css_class(PRESSED);
                }
            }
        }
    }

    /// Clears every press, for example when the page is left.
    pub fn clear_presses(&self) {
        self.show_press(&Hotspot::ALL, false);
    }

    /// The action picker for `control`: the tasks the mouse allows, one
    /// click applies.
    pub fn open_picker(self: &Rc<Self>, control: RemapControl) {
        let Some(status) = self.status.borrow().clone() else {
            return;
        };
        self.mouse.set_selected(Some(remaps::hotspot(control)));
        let name = remaps::button_name(control);
        let dialog = adw::Dialog::builder()
            .title(&name)
            .content_width(440)
            .content_height(560)
            .build();
        let header = adw::HeaderBar::builder()
            .title_widget(&adw::WindowTitle::new(
                &name,
                &gettext("Choose what it does"),
            ))
            .build();
        let search = gtk::SearchEntry::builder()
            .placeholder_text(gettext("Search actions"))
            .build();
        let group = adw::PreferencesGroup::builder()
            .title(gettext("Mouse actions"))
            .build();
        let current = remaps::current_target(&status, control);
        let targets = remaps::targets(&status, control);
        let mut rows = Vec::new();
        for &t in &targets {
            let title = if t == control.cid() {
                // TRANSLATORS: an action name followed by this mark.
                format!("{} {}", remaps::action_name(t), gettext("(default)"))
            } else {
                remaps::action_name(t)
            };
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&title))
                .subtitle(glib::markup_escape_text(&remaps::action_description(t)))
                .activatable(true)
                .build();
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_visible(current == Some(t));
            check.update_property(&[gtk::accessible::Property::Label(&gettext("Current"))]);
            row.add_suffix(&check);
            let (sink, d) = (self.sink.clone(), dialog.clone());
            row.connect_activated(move |_| {
                d.close();
                sink(Request::Remap(control, t));
            });
            group.add(&row);
            rows.push((t, row));
        }
        search.connect_search_changed(move |entry| {
            let query = entry.text();
            for (t, row) in &rows {
                row.set_visible(remaps::matches(&query, *t));
            }
        });
        let note = gtk::Label::builder()
            .label(gettext(
                "Keyboard shortcuts, media keys and GNOME actions come with gestures in a later version.",
            ))
            .css_classes(["dim-label", "caption"])
            .wrap(true)
            .xalign(0.0)
            .build();
        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(18)
            .margin_start(12)
            .margin_end(12)
            .build();
        column.append(&search);
        if targets.is_empty() {
            column.append(
                &adw::StatusPage::builder()
                    .icon_name("dialog-information-symbolic")
                    .title(gettext("No actions to choose"))
                    .description(gettext(
                        "The mouse did not say which actions this button allows.",
                    ))
                    .build(),
            );
        } else {
            column.append(&group);
        }
        column.append(&note);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&header);
        view.set_content(Some(
            &gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .child(&column)
                .build(),
        ));
        dialog.set_child(Some(&view));
        dialog.present(Some(&self.root));
        search.grab_focus();
    }
}
