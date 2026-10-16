use easel_core::geometry::Size;

pub fn size_from_pixels(width: f32, height: f32) -> Size {
    Size {
        w: width.max(0.0) as u32,
        h: height.max(0.0) as u32,
    }
}
