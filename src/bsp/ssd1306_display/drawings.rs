use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};

pub const WIDTH: usize = 128;
pub const HEIGHT: usize = 64;
pub type Frame = [u8; WIDTH * HEIGHT / 8];

const fn in_circle(x: i32, y: i32, cx: i32, cy: i32, r: i32) -> bool {
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= r * r + r // `+ r` rounds the edge, no lone pixels at the poles
}

const fn in_ring(x: i32, y: i32, cx: i32, cy: i32, r_in: i32, r_out: i32) -> bool {
    in_circle(x, y, cx, cy, r_out) && !in_circle(x, y, cx, cy, r_in)
}

const fn near_segment(x: i32, y: i32, x0: i32, y0: i32, x1: i32, y1: i32, half_width: i32) -> bool {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let (px, py) = (x - x0, y - y0);
    let len2 = dx * dx + dy * dy;
    let dot = px * dx + py * dy;
    if dot < 0 || dot > len2 {
        return false;
    }
    let cross = px * dy - py * dx;
    cross * cross <= half_width * half_width * len2
}

const fn angry_pixel(x: i32, y: i32) -> bool {
    let face = in_circle(x, y, 64, 32, 30);
    let eyes = in_circle(x, y, 52, 32, 5) || in_circle(x, y, 76, 32, 5);
    let brows = near_segment(x, y, 40, 16, 59, 25, 2) || near_segment(x, y, 88, 16, 69, 25, 2);
    let frown = in_ring(x, y, 64, 58, 10, 13) && y <= 51;
    face && !(eyes || brows || frown)
}

pub const fn angry_face() -> Frame {
    let mut fb = [0u8; WIDTH * HEIGHT / 8];
    let mut y = 0;
    while y < HEIGHT {
        let mut x = 0;
        while x < WIDTH {
            if angry_pixel(x as i32, y as i32) {
                fb[(y / 8) * WIDTH + x] |= 1 << (y % 8);
            }
            x += 1;
        }
        y += 1;
    }
    fb
}

pub struct FrameBuffer(pub Frame);

impl FrameBuffer {
    pub const fn new() -> Self {
        Self([0; WIDTH * HEIGHT / 8])
    }
}

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for FrameBuffer {
    type Color = BinaryColor;
    type Error = core::convert::Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Self::Error> {
        for Pixel(Point { x, y }, color) in pixels {
            if (0..WIDTH as i32).contains(&x) && (0..HEIGHT as i32).contains(&y) {
                let (x, y) = (x as usize, y as usize);
                let byte = &mut self.0[(y / 8) * WIDTH + x];
                let bit = 1 << (y % 8);
                if color.is_on() {
                    *byte |= bit
                } else {
                    *byte &= !bit
                }
            }
        }
        Ok(())
    }
}

pub static ANGRY_FACE: Frame = angry_face();
