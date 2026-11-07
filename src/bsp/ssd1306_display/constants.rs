
pub const SSD1306_I2C_ADDR: u8 = 0x3C;

pub const SSD1306_WIDTH: usize = 128;
pub const SSD1306_HEIGHT: usize = 64;

pub const SSD1306_BUFFER_SIZE: usize = SSD1306_WIDTH  * SSD1306_HEIGHT  / 8;

pub const SSD1306_CONTROL_BYTE_DATA: u8 = 0x40; // control byte: data follows

pub const SSD1306_SET_FULL_WINDOW: [u8; 7] = [
    0x00, // control byte: commands follow
    0x21, 0x00, SSD1306_WIDTH as u8 - 1, // column window 0..=127 (survives MCU reset, so always set it)
    0x22, 0x00, SSD1306_HEIGHT as u8 / 8 - 1, // page window 0..=7; also moves the write pointer to the top-left
];