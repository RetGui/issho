/// An axis-aligned bounding rectangle for an accessibility element.
///
/// Coordinates are relative to the top-left corner of the associated window's
/// client area. A rectangle with a non-positive width or height is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AccessRect {
    /// The horizontal coordinate of the rectangle's left edge.
    pub x: f64,

    /// The vertical coordinate of the rectangle's top edge.
    pub y: f64,

    /// The horizontal extent of the rectangle.
    pub width: f64,

    /// The vertical extent of the rectangle.
    pub height: f64,
}

impl AccessRect {
    /// Creates a rectangle from its top-left position and dimensions.
    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns whether the point `(x, y)` is inside this rectangle.
    ///
    /// The left and top edges are inclusive, while the right and bottom edges
    /// are exclusive. Empty rectangles never contain a point.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        self.width > 0.0
            && self.height > 0.0
            && x >= self.x
            && x < self.x + self.width
            && y >= self.y
            && y < self.y + self.height
    }
}
