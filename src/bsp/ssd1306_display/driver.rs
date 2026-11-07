use core::marker::PhantomData;

use rtic_sync::arbiter::Arbiter;
use crate::i2c::I2cBus;

use crate::{
    bsp::{I2CPeripheral, Initialized, Uninitialized, ssd1306_display::{constants::{SSD1306_I2C_ADDR, SSD1306_SET_FULL_WINDOW}, drawings}}, i2c,
};

pub struct SSD1306Display<State> {
    _state: PhantomData<State>,
    inner: I2CPeripheral,
}

impl SSD1306Display<Uninitialized> {
    async fn configure_display(&self) -> Result<(), crate::i2c::I2CError> {
        self.inner
            .write(&[
                0x00, // control byte: commands follow
                0xAE, // display off while configuring
                0xD5, 0x80, // clock divider / oscillator frequency (default)
                0xA8, 0x3F, // multiplex ratio = 64 rows - 1
                0xD3, 0x00, // display offset = 0
                0x40, // display start line = 0
                0x8D, 0x14, // charge pump ON (needed when powered from 3.3 V)
                0x20, 0x00, // memory mode: horizontal addressing
                0x21, 0x00,
                0x7F, // column window 0..=127 (survives MCU reset, so always set it)
                0x22, 0x00,
                0x07, // page window 0..=7; also moves the write pointer to the top-left
                0xA1, // segment remap: column 127 -> SEG0 (mirror left/right)
                0xC8, // COM scan direction: remapped (mirror top/bottom)
                0xDA, 0x12, // COM pins config for 128x64
                0x81, 0xCF, // contrast
                0xD9, 0xF1, // pre-charge period
                0xDB, 0x40, // VCOMH deselect level
                0xA4, // show RAM contents (0xA5 = all pixels on, handy for testing)
                0xA6, // normal, not inverted (0xA7 = inverted)
                0x2E, // scrolling off
                0xAF, // display on
            ])
            .await?;

        self.inner.write(&drawings::ANGRY_FACE).await?;
        Ok(())
    }

    pub fn new(bus: &'static Arbiter<I2cBus>) -> Self {
        Self {
            _state: PhantomData::<Uninitialized>,
            inner: I2CPeripheral::new(bus, SSD1306_I2C_ADDR),
        }
    }

    pub async fn init(self) -> Result<SSD1306Display<Initialized>, i2c::I2CError> {
        defmt::info!("Display: initializing");
        self.configure_display().await?;
        Ok(SSD1306Display {
            _state: PhantomData::<Initialized>,
            inner: self.inner,
        })
    }
}

impl SSD1306Display<Initialized> {
    pub async fn draw(&self, frame: &drawings::Frame) -> Result<(), i2c::I2CError> {
        self.inner
            .write(&SSD1306_SET_FULL_WINDOW)
            .await?;
        self.inner.write(frame).await?;
        Ok(())
    }
}
