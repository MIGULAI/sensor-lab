use core::marker::PhantomData;

use rtic_monotonics::systick::prelude::*;
use rtic_sync::arbiter::Arbiter;
use stm32f4::stm32f411::I2C1;

use crate::{
    Mono,
    bsp::{Initialized, Uninitialized, ssd1306_display::drawings},
    i2c,
};

pub struct SSD1306Display<'a, State> {
    _state: PhantomData<State>,
    pub bus: &'a Arbiter<I2C1>,
}

impl<'a, State> SSD1306Display<'a, State> {
    async fn call<const N: usize>(
        &self,
        command: u8,
        delay_ms: u32,
    ) -> Result<[u8; N], crate::i2c::I2CError> {
        let result = crate::i2c::write_byte(&*self.bus.access().await, 0x3C, command);
        if let Err(e) = result {
            defmt::error!("Display: write error: {:?}", e);
            return Err(e);
        } else {
            Mono::delay(delay_ms.millis()).await;
            let mut buf = [0u8; N];
            match i2c::read(&*self.bus.access().await, 0x3C, &mut buf) {
                Ok(()) => {
                    return Ok(buf);
                }
                Err(e) => {
                    defmt::error!("Display: read error: {:?}", e);
                    return Err(e);
                }
            }
        }
    }
}

impl<'a> SSD1306Display<'a, Uninitialized> {
    async fn test_display(&self) -> Result<(), crate::i2c::I2CError> {
        match i2c::write(
            &*self.bus.access().await,
            0x3C,
            &[
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
            ],
        ) {
            Ok(()) => {
                defmt::info!("Display: write successful");
            }
            Err(e) => {
                defmt::error!("Display: write error: {:?}", e);
                return Err(e);
            }
        }
        let mut buf = [0u8; 1025];
        buf[0] = 0x40;
        buf[1..].copy_from_slice(&drawings::ANGRY_FACE);
        i2c::write(&*self.bus.access().await, 0x3C, &buf)?;
        Ok(())
    }

    pub fn new(bus: &'a Arbiter<I2C1>) -> Self {
        Self {
            _state: PhantomData::<Uninitialized>,
            bus,
        }
    }

    pub async fn init(self) -> Result<SSD1306Display<'a, Initialized>, i2c::I2CError> {
        defmt::info!("Display: initializing");
        self.test_display().await?;
        Ok(SSD1306Display {
            _state: PhantomData::<Initialized>,
            bus: self.bus,
        })
    }
}

impl<'a> SSD1306Display<'a, Initialized> {
    pub async fn draw(&self, frame: &drawings::Frame) -> Result<(), i2c::I2CError> {
        let bus = &*self.bus.access().await; // hold the bus for window + pixels together
        i2c::write(bus, 0x3C, &[0x00, 0x21, 0x00, 0x7F, 0x22, 0x00, 0x07])?;
        let mut buf = [0u8; 1025];
        buf[0] = 0x40;
        buf[1..].copy_from_slice(frame);
        i2c::write(bus, 0x3C, &buf)
    }
}
