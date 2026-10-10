//! Platform-independent geometry for placing the sidebar next to a target window.
//! All values are physical pixels in virtual-screen coordinates.

// Docking beside a window is Windows-only for now; the geometry stays tested everywhere.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    /// Outside the target, touching its right edge.
    Right,
    /// Outside the target, touching its left edge.
    Left,
    /// Overlapping the target's right edge, used when there is no room outside.
    Inside,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub rect: Rect,
    pub side: Side,
}

/// Places a sidebar of `width` beside `target`, inside the monitor `work` area.
///
/// Tries the right side first, then the left. When neither fits (for example a
/// maximized target) the sidebar overlaps the target's right edge. Vertically it
/// matches the target's visible span, grown to at least `min_height`.
pub fn place_beside(target: Rect, work: Rect, width: i32, min_height: i32) -> Placement {
    let width = width.clamp(1, work.width().max(1));
    let (top, bottom) = vertical_span(target, work, min_height);

    let (left, side) = if target.right + width <= work.right {
        (target.right, Side::Right)
    } else if target.left - width >= work.left {
        (target.left - width, Side::Left)
    } else {
        let left = target.right.min(work.right) - width;
        (left.max(work.left), Side::Inside)
    };

    Placement {
        rect: Rect {
            left,
            top,
            right: left + width,
            bottom,
        },
        side,
    }
}

/// Full-height placement at the right edge of the monitor work area.
pub fn place_at_edge(work: Rect, width: i32) -> Rect {
    let width = width.clamp(1, work.width().max(1));
    Rect {
        left: work.right - width,
        top: work.top,
        right: work.right,
        bottom: work.bottom,
    }
}

/// Converts a logical size to physical pixels for the given scale factor.
pub fn to_physical(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

fn vertical_span(target: Rect, work: Rect, min_height: i32) -> (i32, i32) {
    let min_height = min_height.clamp(1, work.height().max(1));
    let mut top = target.top.max(work.top);
    let mut bottom = target.bottom.min(work.bottom);

    if bottom - top < min_height {
        bottom = (top + min_height).min(work.bottom);
        top = bottom - min_height;
    }
    (top, bottom)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn places_on_the_right_when_there_is_room() {
        let p = place_beside(rect(100, 100, 1200, 900), WORK, 360, 300);
        assert_eq!(p.side, Side::Right);
        assert_eq!(p.rect, rect(1200, 100, 1560, 900));
    }

    #[test]
    fn falls_back_to_the_left() {
        let p = place_beside(rect(700, 100, 1800, 900), WORK, 360, 300);
        assert_eq!(p.side, Side::Left);
        assert_eq!(p.rect, rect(340, 100, 700, 900));
    }

    #[test]
    fn overlaps_a_maximized_target() {
        let p = place_beside(WORK, WORK, 360, 300);
        assert_eq!(p.side, Side::Inside);
        assert_eq!(p.rect, rect(1560, 0, 1920, 1040));
    }

    #[test]
    fn clips_to_the_work_area_vertically() {
        let p = place_beside(rect(100, -50, 1200, 1100), WORK, 360, 300);
        assert_eq!((p.rect.top, p.rect.bottom), (0, 1040));
    }

    #[test]
    fn grows_short_targets_to_min_height() {
        let p = place_beside(rect(100, 900, 1200, 1000), WORK, 360, 300);
        assert_eq!((p.rect.top, p.rect.bottom), (740, 1040));
    }

    #[test]
    fn inside_placement_stays_on_screen_when_target_overflows() {
        let p = place_beside(rect(-200, 0, 2200, 1040), WORK, 360, 300);
        assert_eq!(p.side, Side::Inside);
        assert_eq!(p.rect.right, 1920);
    }

    #[test]
    fn works_on_a_secondary_monitor_with_negative_coordinates() {
        let work = rect(-2560, 0, 0, 1400);
        let p = place_beside(rect(-2400, 100, -1000, 1300), work, 540, 450);
        assert_eq!(p.side, Side::Right);
        assert_eq!(p.rect, rect(-1000, 100, -460, 1300));
    }

    #[test]
    fn edge_placement_uses_full_work_height() {
        assert_eq!(place_at_edge(WORK, 360), rect(1560, 0, 1920, 1040));
    }

    #[test]
    fn converts_logical_to_physical() {
        assert_eq!(to_physical(360.0, 1.5), 540);
        assert_eq!(to_physical(360.0, 1.25), 450);
    }
}
