use crate::AccessRect;

use objc2_foundation::{NSPoint, NSRect, NSSize};

pub(super) fn view_rect(rect: AccessRect, bounds: NSRect, scale: f64, flipped: bool) -> NSRect {
    let x = bounds.origin.x + rect.x / scale;
    let y = if flipped {
        bounds.origin.y + rect.y / scale
    } else {
        bounds.origin.y + bounds.size.height - (rect.y + rect.height) / scale
    };
    NSRect::new(
        NSPoint::new(x, y),
        NSSize::new(rect.width / scale, rect.height / scale),
    )
}

pub(super) fn tree_point(point: NSPoint, bounds: NSRect, scale: f64, flipped: bool) -> (f64, f64) {
    let x = (point.x - bounds.origin.x) * scale;
    let y = if flipped {
        point.y - bounds.origin.y
    } else {
        bounds.origin.y + bounds.size.height - point.y
    } * scale;
    (x, y)
}
