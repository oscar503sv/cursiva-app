//! `CursivaMouseView`: the mouse illustration drawn with GskPath, with
//! hover, selected, pressed and disabled parts, keyboard focus, and light
//! and dark colors. Phase 5d displays it; choosing an action comes in 5f.

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashSet;
use std::sync::OnceLock;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib::subclass::Signal;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

use crate::geometry::{self, BODY, CREASE, Hotspot};

/// Spoken and tooltip name of a part.
pub fn hotspot_name(h: Hotspot) -> String {
    match h {
        Hotspot::Left => gettext("Left button"),
        Hotspot::Right => gettext("Right button"),
        Hotspot::Wheel => gettext("Main wheel and middle button"),
        Hotspot::Mode => gettext("Wheel mode button"),
        Hotspot::Thumb => gettext("Thumb wheel"),
        Hotspot::Forward => gettext("Forward button"),
        Hotspot::Back => gettext("Back button"),
        Hotspot::Gesture => gettext("Gesture button"),
    }
}

struct Paths {
    body: gsk::Path,
    crease: gsk::Path,
    parts: Vec<(Hotspot, gsk::Path, Option<gsk::Path>)>,
}

impl Paths {
    fn build() -> Paths {
        let parse = |d: &str| gsk::Path::parse(d).expect("the mouse path data is valid");
        Paths {
            body: parse(BODY),
            crease: parse(CREASE),
            parts: Hotspot::ALL
                .iter()
                .map(|&h| (h, parse(&h.outline()), h.ridges().map(parse)))
                .collect(),
        }
    }
}

struct Palette {
    body: gdk::RGBA,
    part: gdk::RGBA,
    line: gdk::RGBA,
    ridge: gdk::RGBA,
    hover: gdk::RGBA,
    selected: gdk::RGBA,
    pressed: gdk::RGBA,
    focus: gdk::RGBA,
}

impl Palette {
    fn current(widget: &gtk::Widget) -> Palette {
        let style = adw::StyleManager::default();
        Palette::new(style.is_dark(), style.accent_color_rgba(), widget.color())
    }

    /// Colors for a light or dark style, an accent and the text color.
    fn new(dark: bool, accent: gdk::RGBA, fg: gdk::RGBA) -> Palette {
        let rgb = |r: u8, g: u8, b: u8| {
            gdk::RGBA::new(
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
                1.0,
            )
        };
        let (body, part, pressed) = if dark {
            (
                rgb(0x3a, 0x3a, 0x40),
                rgb(0x4a, 0x4a, 0x52),
                rgb(0xf5, 0xc2, 0x11),
            )
        } else {
            (
                rgb(0xf4, 0xf4, 0xf7),
                rgb(0xe6, 0xe6, 0xec),
                rgb(0xf6, 0xd3, 0x2d),
            )
        };
        Palette {
            body,
            part,
            line: with_alpha(fg, 0.45),
            ridge: with_alpha(fg, 0.25),
            hover: with_alpha(accent, 0.35),
            selected: accent,
            pressed,
            focus: accent,
        }
    }
}

fn with_alpha(c: gdk::RGBA, alpha: f32) -> gdk::RGBA {
    gdk::RGBA::new(c.red(), c.green(), c.blue(), alpha)
}

/// What to highlight.
struct State<'a> {
    hover: Option<Hotspot>,
    focused: Option<Hotspot>,
    selected: Option<Hotspot>,
    pressed: &'a HashSet<Hotspot>,
    disabled: &'a HashSet<Hotspot>,
}

/// Draws the mouse fitted in `size`, as `mouse.svg` does.
fn draw(snapshot: &gtk::Snapshot, paths: &Paths, p: &Palette, size: (f32, f32), st: &State) {
    let (scale, dx, dy) = geometry::fit(size.0, size.1);
    if scale <= 0.0 {
        return;
    }
    snapshot.save();
    snapshot.translate(&graphene::Point::new(dx, dy));
    snapshot.scale(scale, scale);

    let stroke = |width: f32| {
        let s = gsk::Stroke::new(width);
        s.set_line_join(gsk::LineJoin::Round);
        s.set_line_cap(gsk::LineCap::Round);
        s
    };
    snapshot.append_fill(&paths.body, gsk::FillRule::Winding, &p.body);
    snapshot.append_stroke(&paths.body, &stroke(2.0), &p.line);
    snapshot.append_stroke(&paths.crease, &stroke(1.2), &p.ridge);

    for (spot, outline, ridges) in &paths.parts {
        let spot = *spot;
        let off = st.disabled.contains(&spot);
        let fill = if st.pressed.contains(&spot) {
            p.pressed
        } else if st.selected == Some(spot) {
            p.selected
        } else {
            p.part
        };
        if off {
            // Faded, like `.m-off` in the mockup.
            snapshot.push_opacity(0.55);
        }
        snapshot.append_fill(outline, gsk::FillRule::Winding, &fill);
        if !off && fill == p.part && (st.hover == Some(spot) || st.focused == Some(spot)) {
            snapshot.append_fill(outline, gsk::FillRule::Winding, &p.hover);
        }
        if st.focused == Some(spot) {
            snapshot.append_stroke(outline, &stroke(2.5), &p.focus);
        } else {
            snapshot.append_stroke(outline, &stroke(1.5), &p.line);
        }
        if let Some(r) = ridges {
            snapshot.append_stroke(r, &stroke(2.0), &p.ridge);
        }
        if off {
            snapshot.pop();
        }
    }
    snapshot.restore();
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MouseView {
        pub(super) paths: OnceCell<Paths>,
        pub hover: Cell<Option<Hotspot>>,
        /// The part that has keyboard focus while the widget is focused.
        pub cursor: Cell<Option<Hotspot>>,
        pub selected: Cell<Option<Hotspot>>,
        pub pressed: RefCell<HashSet<Hotspot>>,
        pub disabled: RefCell<HashSet<Hotspot>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MouseView {
        const NAME: &'static str = "CursivaMouseView";
        type Type = super::MouseView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("mouseview");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for MouseView {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                // A part was clicked or chosen with the keyboard; the
                // argument is its id from `geometry::Hotspot::id`.
                vec![
                    Signal::builder("hotspot-activated")
                        .param_types([String::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_focusable(true);
            obj.set_has_tooltip(true);
            obj.update_property(&[gtk::accessible::Property::Label(&gettext("Mouse"))]);

            let motion = gtk::EventControllerMotion::new();
            motion.connect_motion(glib::clone!(
                #[weak]
                obj,
                move |_, x, y| obj.imp().set_hover(obj.hit(x, y))
            ));
            motion.connect_leave(glib::clone!(
                #[weak]
                obj,
                move |_| obj.imp().set_hover(None)
            ));
            obj.add_controller(motion);

            let click = gtk::GestureClick::new();
            click.connect_released(glib::clone!(
                #[weak]
                obj,
                move |_, _, x, y| {
                    if let Some(h) = obj.hit(x, y) {
                        obj.imp().cursor.set(Some(h));
                        obj.activate(h);
                    }
                }
            ));
            obj.add_controller(click);

            let keys = gtk::EventControllerKey::new();
            keys.connect_key_pressed(glib::clone!(
                #[weak]
                obj,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, key, _, _| obj.imp().key(key)
            ));
            obj.add_controller(keys);

            let focus = gtk::EventControllerFocus::new();
            focus.connect_enter(glib::clone!(
                #[weak]
                obj,
                move |_| {
                    let imp = obj.imp();
                    if imp.cursor.get().is_none() {
                        imp.cursor.set(imp.selected.get().or(Some(Hotspot::ALL[0])));
                    }
                    imp.announce();
                    obj.queue_draw();
                }
            ));
            focus.connect_leave(glib::clone!(
                #[weak]
                obj,
                move |_| obj.queue_draw()
            ));
            obj.add_controller(focus);

            let style = adw::StyleManager::default();
            style.connect_dark_notify(glib::clone!(
                #[weak]
                obj,
                move |_| obj.queue_draw()
            ));
            style.connect_accent_color_rgba_notify(glib::clone!(
                #[weak]
                obj,
                move |_| obj.queue_draw()
            ));
        }
    }

    impl WidgetImpl for MouseView {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => (160, 240, -1, -1),
                _ => (220, 330, -1, -1),
            }
        }

        fn query_tooltip(&self, x: i32, y: i32, keyboard: bool, tooltip: &gtk::Tooltip) -> bool {
            let h = if keyboard {
                self.cursor.get()
            } else {
                self.obj().hit(f64::from(x), f64::from(y))
            };
            h.map(|h| tooltip.set_text(Some(&hotspot_name(h))))
                .is_some()
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let state = State {
                hover: self.hover.get(),
                focused: obj.has_focus().then(|| self.cursor.get()).flatten(),
                selected: self.selected.get(),
                pressed: &self.pressed.borrow(),
                disabled: &self.disabled.borrow(),
            };
            draw(
                snapshot,
                self.paths.get_or_init(Paths::build),
                &Palette::current(obj.upcast_ref()),
                (obj.width() as f32, obj.height() as f32),
                &state,
            );
        }
    }

    impl MouseView {
        fn set_hover(&self, h: Option<Hotspot>) {
            if self.hover.replace(h) != h {
                let obj = self.obj();
                let enabled = h.is_some_and(|h| !self.disabled.borrow().contains(&h));
                obj.set_cursor_from_name(enabled.then_some("pointer"));
                obj.queue_draw();
            }
        }

        fn key(&self, key: gdk::Key) -> glib::Propagation {
            let all = &Hotspot::ALL;
            let at = self
                .cursor
                .get()
                .and_then(|c| all.iter().position(|&h| h == c))
                .unwrap_or(0);
            let next = match key {
                gdk::Key::Right | gdk::Key::Down => all[(at + 1) % all.len()],
                gdk::Key::Left | gdk::Key::Up => all[(at + all.len() - 1) % all.len()],
                gdk::Key::Home => all[0],
                gdk::Key::End => all[all.len() - 1],
                gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::space => {
                    self.obj().activate(all[at]);
                    return glib::Propagation::Stop;
                }
                _ => return glib::Propagation::Proceed,
            };
            self.cursor.set(Some(next));
            self.announce();
            self.obj().queue_draw();
            glib::Propagation::Stop
        }

        /// Tells assistive technology which part has the keyboard focus.
        fn announce(&self) {
            if let Some(h) = self.cursor.get() {
                self.obj()
                    .update_property(&[gtk::accessible::Property::Description(&hotspot_name(h))]);
            }
        }
    }
}

glib::wrapper! {
    pub struct MouseView(ObjectSubclass<imp::MouseView>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for MouseView {
    fn default() -> Self {
        Self::new()
    }
}

impl MouseView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// The part under a widget point, if any; disabled parts count too, so
    /// their tooltip still says what they are.
    pub fn hit(&self, x: f64, y: f64) -> Option<Hotspot> {
        let (vx, vy) = geometry::to_view(
            x as f32,
            y as f32,
            self.width() as f32,
            self.height() as f32,
        )?;
        let point = graphene::Point::new(vx, vy);
        let paths = self.imp().paths.get_or_init(Paths::build);
        paths
            .parts
            .iter()
            .rev()
            .find(|(_, outline, _)| outline.in_fill(&point, gsk::FillRule::Winding))
            .map(|(h, _, _)| *h)
    }

    pub fn selected(&self) -> Option<Hotspot> {
        self.imp().selected.get()
    }

    pub fn set_selected(&self, h: Option<Hotspot>) {
        self.imp().selected.set(h);
        self.queue_draw();
    }

    /// Lights a part while its button is held (used from phase 5f).
    pub fn set_pressed(&self, h: Hotspot, pressed: bool) {
        let mut set = self.imp().pressed.borrow_mut();
        let changed = if pressed {
            set.insert(h)
        } else {
            set.remove(&h)
        };
        drop(set);
        if changed {
            self.queue_draw();
        }
    }

    /// Greys out a part that cannot be configured.
    pub fn set_disabled(&self, h: Hotspot, disabled: bool) {
        let mut set = self.imp().disabled.borrow_mut();
        if disabled {
            set.insert(h)
        } else {
            set.remove(&h)
        };
        drop(set);
        self.queue_draw();
    }

    fn activate(&self, h: Hotspot) {
        if self.imp().disabled.borrow().contains(&h) {
            return;
        }
        self.set_selected(Some(h));
        self.emit_by_name::<()>("hotspot-activated", &[&h.id().to_owned()]);
    }

    pub fn connect_hotspot_activated<F: Fn(&Self, &str) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "hotspot-activated",
            false,
            glib::closure_local!(move |view: &MouseView, id: String| f(view, &id)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// gsk parses every path without a display, and the hit areas match
    /// the drawing: each part's centre lies in its own outline.
    #[test]
    fn paths_parse_and_hit_their_own_part() {
        let paths = Paths::build();
        let centres = [
            (Hotspot::Left, 130.0, 110.0),
            (Hotspot::Right, 215.0, 110.0),
            (Hotspot::Wheel, 169.0, 88.0),
            (Hotspot::Mode, 170.0, 180.0),
            (Hotspot::Thumb, 121.5, 270.0),
            (Hotspot::Forward, 96.0, 255.0),
            (Hotspot::Back, 100.0, 285.0),
            (Hotspot::Gesture, 60.0, 318.0),
        ];
        for (spot, x, y) in centres {
            let point = graphene::Point::new(x, y);
            let hits: Vec<Hotspot> = paths
                .parts
                .iter()
                .filter(|(_, o, _)| o.in_fill(&point, gsk::FillRule::Winding))
                .map(|(h, _, _)| *h)
                .collect();
            assert_eq!(hits, [spot], "at ({x}, {y})");
        }
        assert!(
            paths
                .body
                .in_fill(&graphene::Point::new(200.0, 300.0), gsk::FillRule::Winding)
        );
        assert!(
            !paths
                .body
                .in_fill(&graphene::Point::new(5.0, 5.0), gsk::FillRule::Winding)
        );
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;

    /// Renders the illustration offscreen with GSK's Cairo renderer; no
    /// window is shown. Skipped when there is no display to initialize GTK
    /// with. With `CURSIVA_RENDER_DIR` set, the images are saved there for
    /// a visual check.
    #[test]
    fn renders_light_and_dark_offscreen() {
        if gtk::init().is_err() {
            eprintln!("no display: rendering test skipped");
            return;
        }
        let paths = Paths::build();
        let accent = gdk::RGBA::new(0.208, 0.518, 0.894, 1.0);
        let none = HashSet::new();
        let pressed: HashSet<Hotspot> = [Hotspot::Back].into();
        let disabled: HashSet<Hotspot> = [Hotspot::Gesture].into();
        let cases = [
            ("light", false, gdk::RGBA::new(0.0, 0.0, 0.0, 0.8)),
            ("dark", true, gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)),
        ];
        let renderer = gsk::CairoRenderer::new();
        renderer
            .realize(None::<&gdk::Surface>)
            .expect("cairo renderer realizes without a surface");
        for (name, dark, fg) in cases {
            let snapshot = gtk::Snapshot::new();
            let st = State {
                hover: Some(Hotspot::Wheel),
                focused: Some(Hotspot::Thumb),
                selected: Some(Hotspot::Forward),
                pressed: if dark { &pressed } else { &none },
                disabled: &disabled,
            };
            draw(
                &snapshot,
                &paths,
                &Palette::new(dark, accent, fg),
                (320.0, 440.0),
                &st,
            );
            let node = snapshot.to_node().expect("something was drawn");
            let texture =
                renderer.render_texture(&node, Some(&graphene::Rect::new(0.0, 0.0, 320.0, 440.0)));
            assert_eq!((texture.width(), texture.height()), (320, 440));
            if let Some(dir) = std::env::var_os("CURSIVA_RENDER_DIR") {
                let path = std::path::Path::new(&dir).join(format!("mouse-{name}.png"));
                texture.save_to_png(path).unwrap();
            }
        }
        renderer.unrealize();
    }
}
