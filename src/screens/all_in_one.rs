use core::fmt::Write;
use embedded_graphics::{
    mono_font::{MonoTextStyle, iso_8859_1::FONT_10X20},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use heapless::String;

use crate::{bsp::ssd1306_display::drawings::FrameBuffer, state::State};

pub fn render(fb: &mut FrameBuffer, state: &State) {
    let style = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
    let _ = fb.clear(BinaryColor::Off);

    let sht = state.sht40_state.as_ref().and_then(|s| s.measurements.as_ref());
    let bmp = state.bmp280_state.as_ref().and_then(|s| s.measurements.as_ref());

    let mut lines: [String<16>; 3] = Default::default();
    let _ = match sht {
        Some(m) => write!(lines[0], "T {:.1} °C", m.temperature_celsius()),
        None => write!(lines[0], "T --"),
    };
    let _ = match bmp {
        Some(m) => write!(lines[1], "P {:.1} hPa", m.pressure / 100.0), // Pa -> hPa
        None => write!(lines[1], "P --"),
    };
    let _ = match sht {
        Some(m) => write!(lines[2], "H {:.1} %", m.humidity_percent()),
        None => write!(lines[2], "H --"),
    };

    for (i, line) in lines.iter().enumerate() {
        let y = i as i32 * 21; // 20 px font + 1 px gap
        let _ = Text::with_baseline(line, Point::new(0, y), style, Baseline::Top).draw(fb);
    }
}
