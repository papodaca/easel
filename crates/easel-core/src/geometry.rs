use serde::{Deserialize, Serialize};

/// Pixel size of an image or frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

/// Axis-aligned rectangle in image pixel space. `x` and `y` are the top-left
/// corner when `w` and `h` are positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Provider box on the 0–1000 grid, ordered `[top, left, bottom, right]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridBox(pub [u16; 4]);

impl Rect {
    /// Rewrites a negative width or height as the equivalent positive rectangle.
    #[must_use]
    pub fn normalize(self) -> Self {
        let (x, w) = if self.w < 0 {
            (self.x.saturating_add(self.w), self.w.saturating_neg())
        } else {
            (self.x, self.w)
        };
        let (y, h) = if self.h < 0 {
            (self.y.saturating_add(self.h), self.h.saturating_neg())
        } else {
            (self.y, self.h)
        };
        Self { x, y, w, h }
    }

    /// Clips this rectangle to `frame`, after normalizing negative extents.
    #[must_use]
    pub fn clamp_to(self, frame: Size) -> Self {
        let rect = self.normalize();
        let max_x = i32::try_from(frame.w).unwrap_or(i32::MAX);
        let max_y = i32::try_from(frame.h).unwrap_or(i32::MAX);
        let x0 = rect.x.clamp(0, max_x);
        let y0 = rect.y.clamp(0, max_y);
        let x1 = rect.x.saturating_add(rect.w).clamp(0, max_x);
        let y1 = rect.y.saturating_add(rect.h).clamp(0, max_y);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    /// Converts pixel edges to the provider grid `[top, left, bottom, right]`.
    #[must_use]
    pub fn to_grid(self, frame: Size) -> GridBox {
        let rect = self.normalize();
        GridBox([
            scale_to_grid(rect.y, frame.h),
            scale_to_grid(rect.x, frame.w),
            scale_to_grid(rect.y.saturating_add(rect.h), frame.h),
            scale_to_grid(rect.x.saturating_add(rect.w), frame.w),
        ])
    }

    /// Inverse of [`Self::to_grid`]. Rounding can move an edge by a pixel.
    #[must_use]
    pub fn from_grid(grid: GridBox, frame: Size) -> Self {
        let [top, left, bottom, right] = grid.0;
        let x = scale_from_grid(left, frame.w);
        let y = scale_from_grid(top, frame.h);
        let right_px = scale_from_grid(right, frame.w);
        let bottom_px = scale_from_grid(bottom, frame.h);
        Self {
            x,
            y,
            w: right_px - x,
            h: bottom_px - y,
        }
    }
}

fn scale_to_grid(value: i32, span: u32) -> u16 {
    if span == 0 {
        return 0;
    }
    let scaled = (f64::from(value) * 1000.0) / f64::from(span);
    let clamped = scaled.round().clamp(0.0, 1000.0);
    clamped as u16
}

fn scale_from_grid(grid: u16, span: u32) -> i32 {
    let scaled = f64::from(grid) * f64::from(span) / 1000.0;
    scaled.round() as i32
}

#[cfg(test)]
mod tests {
    use super::{GridBox, Rect, Size};

    #[test]
    fn to_grid_matches_the_official_flux_example() {
        let rect = Rect {
            x: 384,
            y: 108,
            w: 1536 - 384,
            h: 972 - 108,
        };
        assert_eq!(
            rect.to_grid(Size { w: 1920, h: 1080 }),
            GridBox([100, 200, 900, 800])
        );
    }

    #[test]
    fn grid_round_trip_stays_within_one_pixel() {
        let square = Rect {
            x: 125,
            y: 40,
            w: 300,
            h: 610,
        };
        assert_within_one_pixel(square, Size { w: 1000, h: 1000 });

        let three_two = Rect {
            x: 90,
            y: 40,
            w: 700,
            h: 380,
        };
        assert_within_one_pixel(three_two, Size { w: 1500, h: 1000 });
    }

    #[test]
    fn negative_width_normalizes_to_the_positive_rect() {
        let rect = Rect {
            x: 100,
            y: 50,
            w: -40,
            h: -10,
        };
        assert_eq!(
            rect.normalize(),
            Rect {
                x: 60,
                y: 40,
                w: 40,
                h: 10,
            }
        );
    }

    fn assert_within_one_pixel(rect: Rect, frame: Size) {
        let back = Rect::from_grid(rect.to_grid(frame), frame);
        assert!(
            (back.x - rect.x).abs() <= 1
                && (back.y - rect.y).abs() <= 1
                && (back.w - rect.w).abs() <= 1
                && (back.h - rect.h).abs() <= 1,
            "round trip {rect:?} -> {back:?} on {frame:?}"
        );
    }
}
