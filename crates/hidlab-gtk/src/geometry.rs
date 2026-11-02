//! The mouse illustration of `docs/mockups/mouse.svg` as SVG path data, in
//! the drawing's own coordinates (a 320 × 440 view box). Original artwork
//! for Cursiva; it is not Logitech artwork.

/// Size of the drawing's view box.
pub const VIEW_WIDTH: f32 = 320.0;
pub const VIEW_HEIGHT: f32 = 440.0;

/// Outline of the mouse body.
pub const BODY: &str = "M170 30 C214 30 246 50 254 108 C262 170 278 260 268 330 \
     C258 392 214 420 168 418 C132 416 108 404 96 388 C72 382 44 370 32 348 \
     C20 326 22 296 34 274 C44 256 60 246 80 240 C80 170 82 110 92 76 \
     C102 44 132 30 170 30 Z";

/// The thumb rest crease, stroked only.
pub const CREASE: &str = "M82 246 C94 300 102 346 102 392";

/// A part of the mouse the user can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Hotspot {
    Left,
    Right,
    /// The main wheel; pressing it is the middle button.
    Wheel,
    /// The button behind the wheel that switches the wheel mode.
    Mode,
    Thumb,
    Forward,
    Back,
    Gesture,
}

impl Hotspot {
    /// Drawing order, which is also keyboard order.
    pub const ALL: [Hotspot; 8] = [
        Hotspot::Left,
        Hotspot::Right,
        Hotspot::Wheel,
        Hotspot::Mode,
        Hotspot::Thumb,
        Hotspot::Forward,
        Hotspot::Back,
        Hotspot::Gesture,
    ];

    /// Stable id, the same as in `mouse.svg` without the `h-` prefix.
    pub fn id(self) -> &'static str {
        match self {
            Hotspot::Left => "left",
            Hotspot::Right => "right",
            Hotspot::Wheel => "wheel",
            Hotspot::Mode => "mode",
            Hotspot::Thumb => "thumb",
            Hotspot::Forward => "forward",
            Hotspot::Back => "back",
            Hotspot::Gesture => "gesture",
        }
    }

    /// Fill outline, SVG path data.
    pub fn outline(self) -> String {
        match self {
            Hotspot::Left => "M165 33 C134 33 108 44 97 76 C89 106 86 140 85 169 \
                 C112 164 140 161 162 160 Z"
                .into(),
            Hotspot::Right => {
                "M175 33 C212 33 240 52 248 108 L253 165 C232 161 205 159 178 160 Z".into()
            }
            Hotspot::Wheel => rounded_rect(157.0, 54.0, 24.0, 68.0, 12.0),
            Hotspot::Mode => rounded_rect(162.0, 170.0, 16.0, 20.0, 6.0),
            Hotspot::Thumb => rounded_rect(111.0, 250.0, 21.0, 40.0, 9.0),
            Hotspot::Forward => rounded_rect(91.0, 242.0, 10.0, 26.0, 5.0),
            Hotspot::Back => rounded_rect(95.0, 272.0, 10.0, 26.0, 5.0),
            Hotspot::Gesture => ellipse(60.0, 318.0, 20.0, 12.0, -20.0),
        }
    }

    /// Ridges drawn over the part, stroked only.
    pub fn ridges(self) -> Option<&'static str> {
        match self {
            Hotspot::Wheel => Some(
                "M163 66 H175 M163 74 H175 M163 82 H175 M163 90 H175 \
                 M163 98 H175 M163 106 H175",
            ),
            Hotspot::Thumb => {
                Some("M116 258 H127 M116 264 H127 M116 270 H127 M116 276 H127 M116 282 H127")
            }
            _ => None,
        }
    }
}

/// A rectangle with corner radius `r`, as path data.
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> String {
    let (x2, y2) = (x + w, y + h);
    format!(
        "M{} {y} H{} A{r} {r} 0 0 1 {x2} {} V{} A{r} {r} 0 0 1 {} {y2} H{} \
         A{r} {r} 0 0 1 {x} {} V{} A{r} {r} 0 0 1 {} {y} Z",
        x + r,
        x2 - r,
        y + r,
        y2 - r,
        x2 - r,
        x + r,
        y2 - r,
        y + r,
        x + r,
    )
}

/// An ellipse centred on (`cx`, `cy`), rotated by `deg` degrees, as two arcs.
fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32, deg: f32) -> String {
    let (s, c) = deg.to_radians().sin_cos();
    let (x1, y1) = (cx + rx * c, cy + rx * s);
    let (x2, y2) = (cx - rx * c, cy - rx * s);
    format!(
        "M{x1:.2} {y1:.2} A{rx} {ry} {deg} 1 1 {x2:.2} {y2:.2} \
         A{rx} {ry} {deg} 1 1 {x1:.2} {y1:.2} Z"
    )
}

/// Scale and offset that fit the drawing in a `width` × `height` area,
/// centred, keeping its proportions: (scale, dx, dy).
pub fn fit(width: f32, height: f32) -> (f32, f32, f32) {
    let scale = (width / VIEW_WIDTH).min(height / VIEW_HEIGHT).max(0.0);
    let dx = (width - VIEW_WIDTH * scale) / 2.0;
    let dy = (height - VIEW_HEIGHT * scale) / 2.0;
    (scale, dx, dy)
}

/// Converts a widget point to drawing coordinates.
pub fn to_view(x: f32, y: f32, width: f32, height: f32) -> Option<(f32, f32)> {
    let (scale, dx, dy) = fit(width, height);
    (scale > 0.0).then(|| ((x - dx) / scale, (y - dy) / scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = Hotspot::ALL.iter().map(|h| h.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Hotspot::ALL.len());
    }

    #[test]
    fn rounded_rect_closes_on_its_start() {
        assert_eq!(
            rounded_rect(10.0, 20.0, 30.0, 40.0, 5.0),
            "M15 20 H35 A5 5 0 0 1 40 25 V55 A5 5 0 0 1 35 60 H15 \
             A5 5 0 0 1 10 55 V25 A5 5 0 0 1 15 20 Z"
        );
    }

    #[test]
    fn gesture_ellipse_runs_along_its_tilted_axis() {
        // The ends of the major axis of mouse.svg's rotate(-20) ellipse.
        assert!(
            Hotspot::Gesture
                .outline()
                .starts_with("M78.79 311.16 A20 12 -20 1 1 41.21 324.84")
        );
    }

    #[test]
    fn fit_centres_and_keeps_proportions() {
        assert_eq!(fit(640.0, 880.0), (2.0, 0.0, 0.0));
        assert_eq!(fit(420.0, 440.0), (1.0, 50.0, 0.0));
        assert_eq!(to_view(50.0, 0.0, 420.0, 440.0), Some((0.0, 0.0)));
        assert_eq!(to_view(1.0, 1.0, 0.0, 0.0), None);
    }
}
