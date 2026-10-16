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

/// A point in image-pixel or view-pixel space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Axis-aligned rectangle in view pixels. Distinct from [`Rect`], which is
/// integer image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Image-to-view transform. `offset` is the view position of the image origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub image_size: Size,
    pub view_size: Size,
    pub scale: f32,
    pub offset: Point,
}

impl Viewport {
    pub const MIN_SCALE: f32 = 0.05;
    pub const MAX_SCALE: f32 = 32.0;
    const MIN_VISIBLE_PX: f32 = 64.0;

    /// Fits the image inside the view, centered, never scaling above 1.
    #[must_use]
    pub fn fit(image_size: Size, view_size: Size) -> Self {
        let mut viewport = Self {
            image_size,
            view_size,
            scale: 1.0,
            offset: Point { x: 0.0, y: 0.0 },
        };
        viewport.scale = fit_scale(image_size, view_size);
        viewport.recenter();
        viewport.clamp();
        viewport
    }

    /// Shows the image at 100% and recenters it.
    pub fn actual_size(&mut self) {
        self.scale = 1.0;
        self.recenter();
        self.clamp();
    }

    /// Multiplies the scale by `factor`, keeping `view_point` on the same image pixel.
    pub fn zoom_at(&mut self, view_point: Point, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 || self.scale <= 0.0 {
            return;
        }
        let image_point = self.view_to_image(view_point);
        self.scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        self.offset = Point {
            x: view_point.x - image_point.x * self.scale,
            y: view_point.y - image_point.y * self.scale,
        };
        self.clamp();
    }

    /// Moves the image by `delta` view pixels, then clamps it on screen.
    pub fn pan_by(&mut self, delta: Point) {
        self.offset.x += delta.x;
        self.offset.y += delta.y;
        self.clamp();
    }

    /// Keeps at least `min(image extent in view, 64px)` of the image inside the view.
    pub fn clamp(&mut self) {
        let rect = self.image_rect_in_view();
        let view_w = self.view_size.w as f32;
        let view_h = self.view_size.h as f32;
        self.offset.x = clamp_offset(self.offset.x, rect.w, view_w);
        self.offset.y = clamp_offset(self.offset.y, rect.h, view_h);
    }

    #[must_use]
    pub fn image_to_view(&self, image_point: Point) -> Point {
        Point {
            x: image_point.x * self.scale + self.offset.x,
            y: image_point.y * self.scale + self.offset.y,
        }
    }

    #[must_use]
    pub fn view_to_image(&self, view_point: Point) -> Point {
        let scale = if self.scale == 0.0 { 1.0 } else { self.scale };
        Point {
            x: (view_point.x - self.offset.x) / scale,
            y: (view_point.y - self.offset.y) / scale,
        }
    }

    #[must_use]
    pub fn image_rect_in_view(&self) -> ViewRect {
        ViewRect {
            x: self.offset.x,
            y: self.offset.y,
            w: self.image_size.w as f32 * self.scale,
            h: self.image_size.h as f32 * self.scale,
        }
    }

    fn recenter(&mut self) {
        let rect_w = self.image_size.w as f32 * self.scale;
        let rect_h = self.image_size.h as f32 * self.scale;
        self.offset = Point {
            x: (self.view_size.w as f32 - rect_w) / 2.0,
            y: (self.view_size.h as f32 - rect_h) / 2.0,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

/// Last matching rectangle wins, so the topmost box is the last one drawn.
#[must_use]
pub fn hit_test(point: Point, boxes: &[ViewRect]) -> Option<usize> {
    boxes.iter().rposition(|rect| contains(*rect, point))
}

#[must_use]
pub fn handle_at(point: Point, rect: ViewRect) -> Option<Handle> {
    const TOLERANCE: f32 = 8.0;
    let anchors = [
        (Handle::NorthWest, rect.x, rect.y),
        (Handle::North, rect.x + rect.w / 2.0, rect.y),
        (Handle::NorthEast, rect.x + rect.w, rect.y),
        (Handle::East, rect.x + rect.w, rect.y + rect.h / 2.0),
        (Handle::SouthEast, rect.x + rect.w, rect.y + rect.h),
        (Handle::South, rect.x + rect.w / 2.0, rect.y + rect.h),
        (Handle::SouthWest, rect.x, rect.y + rect.h),
        (Handle::West, rect.x, rect.y + rect.h / 2.0),
    ];
    anchors
        .into_iter()
        .find(|(_, x, y)| {
            let dx = point.x - x;
            let dy = point.y - y;
            dx * dx + dy * dy <= TOLERANCE * TOLERANCE
        })
        .map(|(handle, _, _)| handle)
}

#[must_use]
pub fn resize_rect(rect: Rect, handle: Handle, to: Point) -> Rect {
    let mut left = rect.x;
    let mut top = rect.y;
    let mut right = rect.x.saturating_add(rect.w);
    let mut bottom = rect.y.saturating_add(rect.h);
    let x = to.x.round() as i32;
    let y = to.y.round() as i32;
    match handle {
        Handle::NorthWest => {
            left = x;
            top = y;
        }
        Handle::North => top = y,
        Handle::NorthEast => {
            right = x;
            top = y;
        }
        Handle::East => right = x,
        Handle::SouthEast => {
            right = x;
            bottom = y;
        }
        Handle::South => bottom = y,
        Handle::SouthWest => {
            left = x;
            bottom = y;
        }
        Handle::West => left = x,
    }
    Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    }
    .normalize()
}

fn contains(rect: ViewRect, point: Point) -> bool {
    point.x >= rect.x
        && point.y >= rect.y
        && point.x <= rect.x + rect.w
        && point.y <= rect.y + rect.h
}

fn fit_scale(image: Size, view: Size) -> f32 {
    if image.w == 0 || image.h == 0 || view.w == 0 || view.h == 0 {
        return 1.0;
    }
    let scale_x = view.w as f32 / image.w as f32;
    let scale_y = view.h as f32 / image.h as f32;
    scale_x.min(scale_y).min(1.0)
}

fn clamp_offset(offset: f32, displayed: f32, view: f32) -> f32 {
    let margin = displayed.min(Viewport::MIN_VISIBLE_PX).min(view.max(0.0));
    let min_offset = margin - displayed;
    let max_offset = view - margin;
    if min_offset > max_offset {
        return (view - displayed) / 2.0;
    }
    offset.clamp(min_offset, max_offset)
}

#[cfg(test)]
mod tests {
    use super::{
        GridBox, Handle, Point, Rect, Size, ViewRect, Viewport, handle_at, hit_test, resize_rect,
    };

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

    fn wide_image() -> Viewport {
        Viewport::fit(Size { w: 4000, h: 2000 }, Size { w: 1000, h: 800 })
    }

    #[test]
    fn fit_centers_a_wide_image_without_cropping() {
        let viewport = wide_image();
        assert!((viewport.scale - 0.25).abs() < 1e-4);
        let rect = viewport.image_rect_in_view();
        assert!((rect.x - 0.0).abs() < 1e-3);
        assert!((rect.y - 150.0).abs() < 1e-3);
        assert!((rect.w - 1000.0).abs() < 1e-3);
        assert!((rect.h - 500.0).abs() < 1e-3);
    }

    #[test]
    fn fit_does_not_upscale_an_image_smaller_than_the_view() {
        let viewport = Viewport::fit(Size { w: 100, h: 50 }, Size { w: 1000, h: 800 });
        assert!((viewport.scale - 1.0).abs() < 1e-4);
        let rect = viewport.image_rect_in_view();
        assert!((rect.x - 450.0).abs() < 1e-3);
        assert!((rect.y - 375.0).abs() < 1e-3);
        assert!((rect.w - 100.0).abs() < 1e-3);
        assert!((rect.h - 50.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_at_keeps_the_image_point_under_the_cursor() {
        let mut viewport = wide_image();
        let cursor = Point { x: 500.0, y: 400.0 };
        let before = viewport.view_to_image(cursor);
        viewport.zoom_at(cursor, 2.0);
        let after = viewport.view_to_image(cursor);
        assert!((before.x - after.x).abs() < 1e-3);
        assert!((before.y - after.y).abs() < 1e-3);
        assert!((viewport.scale - 0.5).abs() < 1e-4);
    }

    #[test]
    fn zoom_clamps_to_the_scale_limits() {
        let mut viewport = wide_image();
        let cursor = Point { x: 100.0, y: 100.0 };
        viewport.zoom_at(cursor, 10_000.0);
        assert!((viewport.scale - 32.0).abs() < 1e-3);
        viewport.zoom_at(cursor, 0.000_01);
        assert!((viewport.scale - 0.05).abs() < 1e-3);
    }

    #[test]
    fn pan_keeps_at_least_64_px_of_the_image_visible() {
        let mut viewport = wide_image();
        viewport.pan_by(Point {
            x: -10_000.0,
            y: 10_000.0,
        });
        let rect = viewport.image_rect_in_view();
        let visible_w = (rect.x + rect.w).min(1000.0) - rect.x.max(0.0);
        let visible_h = (rect.y + rect.h).min(800.0) - rect.y.max(0.0);
        assert!((visible_w - 64.0).abs() < 1e-2, "visible width {visible_w}");
        assert!(
            (visible_h - 64.0).abs() < 1e-2,
            "visible height {visible_h}"
        );
    }

    #[test]
    fn image_and_view_points_are_inverses() {
        let samples = [
            Point { x: 0.0, y: 0.0 },
            Point { x: 12.5, y: 40.0 },
            Point { x: 400.0, y: 250.0 },
            Point {
                x: 3999.0,
                y: 1999.0,
            },
        ];
        for scale in [0.05_f32, 0.25, 1.0, 3.5, 32.0] {
            let viewport = Viewport {
                image_size: Size { w: 4000, h: 2000 },
                view_size: Size { w: 1000, h: 800 },
                scale,
                offset: Point { x: -20.0, y: 15.0 },
            };
            for image_point in samples {
                let view_point = viewport.image_to_view(image_point);
                let back = viewport.view_to_image(view_point);
                assert!(
                    (back.x - image_point.x).abs() < 1e-2 && (back.y - image_point.y).abs() < 1e-2,
                    "scale {scale}: {image_point:?} -> {view_point:?} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn hit_test_returns_the_topmost_box() {
        let boxes = [
            ViewRect {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            ViewRect {
                x: 50.0,
                y: 50.0,
                w: 100.0,
                h: 100.0,
            },
        ];
        assert_eq!(hit_test(Point { x: 60.0, y: 60.0 }, &boxes), Some(1));
        assert_eq!(hit_test(Point { x: 10.0, y: 10.0 }, &boxes), Some(0));
        assert_eq!(hit_test(Point { x: 200.0, y: 10.0 }, &boxes), None);
    }

    #[test]
    fn handle_at_uses_an_eight_pixel_tolerance() {
        let rect = ViewRect {
            x: 100.0,
            y: 100.0,
            w: 80.0,
            h: 40.0,
        };
        assert_eq!(
            handle_at(Point { x: 104.0, y: 96.0 }, rect),
            Some(Handle::NorthWest)
        );
        assert_eq!(
            handle_at(Point { x: 140.0, y: 100.0 }, rect),
            Some(Handle::North)
        );
        assert_eq!(handle_at(Point { x: 140.0, y: 120.0 }, rect), None);
    }

    #[test]
    fn resizing_past_the_opposite_corner_stays_positive() {
        let rect = Rect {
            x: 100,
            y: 80,
            w: 40,
            h: 30,
        };
        let resized = resize_rect(rect, Handle::NorthWest, Point { x: 200.0, y: 180.0 });
        assert!(resized.w > 0 && resized.h > 0);
        assert_eq!(
            resized,
            Rect {
                x: 140,
                y: 110,
                w: 60,
                h: 70,
            }
        );
    }

    #[test]
    fn actual_size_sets_scale_one_and_recenters() {
        let mut viewport = wide_image();
        viewport.zoom_at(Point { x: 10.0, y: 10.0 }, 4.0);
        viewport.actual_size();
        assert!((viewport.scale - 1.0).abs() < 1e-4);
        let rect = viewport.image_rect_in_view();
        assert!((rect.w - 4000.0).abs() < 1e-2);
        assert!((rect.h - 2000.0).abs() < 1e-2);
        assert!((rect.x - ((1000.0 - 4000.0) / 2.0)).abs() < 1e-2);
        assert!((rect.y - ((800.0 - 2000.0) / 2.0)).abs() < 1e-2);
    }
}
