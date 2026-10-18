use crate::geometry::{Point, Size, ViewRect};

/// Gap between tiles, in content pixels. One viewport scale applies to the whole row.
pub const TILE_GAP: f32 = 16.0;

/// Display rectangles for one row, inside [`SheetLayout::content`].
#[derive(Debug, Clone, PartialEq)]
pub struct SheetLayout {
    pub content: Size,
    pub tiles: Vec<ViewRect>,
}

/// A content-space point mapped into one tile's image pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TilePixel {
    pub index: usize,
    pub pixel: Point,
}

/// 1000-wide frame for a concrete `width:height` ratio. `auto` and empty have none.
#[must_use]
pub fn virtual_frame(ratio: Option<&str>) -> Option<Size> {
    let ratio = ratio?;
    if ratio.is_empty() || ratio == "auto" {
        return None;
    }
    let (width, height) = ratio.split_once(':')?;
    let width: u32 = width.parse().ok()?;
    let height: u32 = height.parse().ok()?;
    if width == 0 || height == 0 {
        return None;
    }
    Some(Size {
        w: 1000,
        h: 1000 * height / width,
    })
}

/// Lays sizes left to right at the tallest tile's height, native aspect kept.
#[must_use]
pub fn layout(sizes: &[Size], aspect: Option<&str>) -> SheetLayout {
    let height = sizes
        .iter()
        .filter(|size| size.w > 0 && size.h > 0)
        .map(|size| size.h)
        .max()
        .unwrap_or(0);
    if height == 0 {
        return SheetLayout {
            content: virtual_frame(aspect).unwrap_or(Size { w: 0, h: 0 }),
            tiles: Vec::new(),
        };
    }
    let row_h = height as f32;
    let mut tiles = Vec::with_capacity(sizes.len());
    let mut x = 0.0_f32;
    let mut placed = false;
    for size in sizes {
        if size.w == 0 || size.h == 0 {
            tiles.push(ViewRect {
                x,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            });
            continue;
        }
        if placed {
            x += TILE_GAP;
        }
        let width = row_h * (size.w as f32) / (size.h as f32);
        tiles.push(ViewRect {
            x,
            y: 0.0,
            w: width,
            h: row_h,
        });
        x += width;
        placed = true;
    }
    SheetLayout {
        content: Size {
            w: x.ceil() as u32,
            h: height,
        },
        tiles,
    }
}

/// The tile under `point`, mapped into that image's pixels. A gap or a miss is `None`.
#[must_use]
pub fn hit_tile(layout: &SheetLayout, sizes: &[Size], point: Point) -> Option<TilePixel> {
    let index = layout
        .tiles
        .iter()
        .position(|rect| rect.w > 0.0 && rect.h > 0.0 && contains(*rect, point))?;
    let size = sizes.get(index)?;
    Some(TilePixel {
        index,
        pixel: map_pixel(&layout.tiles[index], size, point),
    })
}

/// Maps `point` into tile `index`, clamping into that tile first.
#[must_use]
pub fn clamp_tile(
    layout: &SheetLayout,
    sizes: &[Size],
    index: usize,
    point: Point,
) -> Option<Point> {
    let rect = layout.tiles.get(index)?;
    let size = sizes.get(index)?;
    if rect.w <= 0.0 || rect.h <= 0.0 || size.w == 0 || size.h == 0 {
        return None;
    }
    let clamped = Point {
        x: point.x.clamp(rect.x, rect.x + rect.w),
        y: point.y.clamp(rect.y, rect.y + rect.h),
    };
    Some(map_pixel(rect, size, clamped))
}

fn map_pixel(rect: &ViewRect, size: &Size, point: Point) -> Point {
    let u = if rect.w == 0.0 {
        0.0
    } else {
        (point.x - rect.x) / rect.w
    };
    let v = if rect.h == 0.0 {
        0.0
    } else {
        (point.y - rect.y) / rect.h
    };
    Point {
        x: u * size.w as f32,
        y: v * size.h as f32,
    }
}

fn contains(rect: ViewRect, point: Point) -> bool {
    point.x >= rect.x
        && point.y >= rect.y
        && point.x <= rect.x + rect.w
        && point.y <= rect.y + rect.h
}

#[cfg(test)]
mod tests {
    use super::{TILE_GAP, clamp_tile, hit_tile, layout};
    use crate::geometry::{Point, Size, Viewport};

    #[test]
    fn two_sizes_share_a_height_and_keep_aspect() {
        let laid = layout(&[Size { w: 100, h: 100 }, Size { w: 400, h: 200 }], None);
        assert_eq!(laid.tiles.len(), 2);
        assert_eq!(laid.tiles[0].h, laid.tiles[1].h);
        assert!((laid.tiles[1].w - laid.tiles[0].w * 2.0).abs() < 0.01);
        assert!((laid.tiles[1].x - (laid.tiles[0].x + laid.tiles[0].w + TILE_GAP)).abs() < 0.01);
        assert!(laid.tiles[0].x < laid.tiles[1].x);
        assert_eq!(laid.tiles[0].w, 200.0);
        assert_eq!(laid.tiles[1].w, 400.0);
        assert_eq!(laid.content, Size { w: 616, h: 200 });
    }

    #[test]
    fn an_empty_row_uses_the_virtual_frame_only_for_a_concrete_ratio() {
        let bare = layout(&[], None);
        assert_eq!(bare.content, Size { w: 0, h: 0 });
        assert!(bare.tiles.is_empty());

        let auto = layout(&[], Some("auto"));
        assert_eq!(auto.content, Size { w: 0, h: 0 });

        let square = layout(&[], Some("1:1"));
        assert_eq!(square.content, Size { w: 1000, h: 1000 });
        assert!(square.tiles.is_empty());

        let wide = layout(&[], Some("16:9"));
        assert_eq!(wide.content, Size { w: 1000, h: 562 });
    }

    #[test]
    fn a_point_maps_into_the_tile_it_hits_and_misses_the_gap() {
        let sizes = [Size { w: 100, h: 100 }, Size { w: 400, h: 200 }];
        let laid = layout(&sizes, None);
        let second = laid.tiles[1];
        let hit = hit_tile(
            &laid,
            &sizes,
            Point {
                x: second.x + 200.0,
                y: 100.0,
            },
        )
        .unwrap();
        assert_eq!(hit.index, 1);
        assert!((hit.pixel.x - 200.0).abs() < 0.01);
        assert!((hit.pixel.y - 100.0).abs() < 0.01);

        let gap = hit_tile(
            &laid,
            &sizes,
            Point {
                x: laid.tiles[0].w + TILE_GAP / 2.0,
                y: 10.0,
            },
        );
        assert!(gap.is_none());

        let past = Point {
            x: laid.tiles[0].w + 1.0,
            y: 10.0,
        };
        assert!(hit_tile(&laid, &sizes, past).is_none());
        let clamped = clamp_tile(&laid, &sizes, 0, past).unwrap();
        assert!((clamped.x - 100.0).abs() < 0.01);
        assert!((clamped.y - 5.0).abs() < 0.01);
    }

    #[test]
    fn fit_centers_the_row_and_does_not_scale_above_one() {
        let laid = layout(&[Size { w: 100, h: 100 }, Size { w: 400, h: 200 }], None);
        let view = Size { w: 800, h: 600 };
        let viewport = Viewport::fit(laid.content, view);
        assert_eq!(viewport.scale, 1.0);
        assert!((viewport.offset.x - 92.0).abs() < 0.01);
        assert!((viewport.offset.y - 200.0).abs() < 0.01);
        let shown = viewport.image_rect_in_view();
        assert!(shown.x >= 0.0 && shown.y >= 0.0);
        assert!(shown.x + shown.w <= view.w as f32 + 0.5);
        assert!(shown.y + shown.h <= view.h as f32 + 0.5);
    }
}
