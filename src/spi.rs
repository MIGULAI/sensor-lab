//! SPI1 on PA5 (SCK) / PA6 (MISO) / PA7 (MOSI), AF5. PA4 = software CS.
//! Mode 0, 8-bit, MSB first, 100 MHz / 128 = 781 kHz.

use crate::gpio::{self, Mode, Port, Pull, Speed};
use stm32f4::stm32f411 as pac;

pub const CS_PIN: u8 = 4;

pub fn init(rcc: &pac::RCC, spi: &pac::SPI1) {
    for pin in [5, 6, 7] {
        gpio::set_alternate(Port::A, pin, 5);
        gpio::configure(Port::A, pin, Mode::Alternate, Pull::None, Speed::VeryHigh, false);
    }
    gpio::configure(Port::A, CS_PIN, Mode::Output, Pull::None, Speed::High, false);
    gpio::set_high(Port::A, CS_PIN);

    rcc.apb2enr().modify(|_, w| w.spi1en().set_bit());
    rcc.apb2rstr().modify(|_, w| w.spi1rst().set_bit());
    rcc.apb2rstr().modify(|_, w| w.spi1rst().clear_bit());

    spi.cr1().write(|w| unsafe {
        w.mstr().set_bit()      // master
            .br().bits(0b110)   // f_PCLK / 128
            .cpol().clear_bit() // mode 0
            .cpha().clear_bit()
            .ssm().set_bit()    // software NSS
            .ssi().set_bit()
            .spe().set_bit()
    });
}

// TODO: transfer_byte(): wait TXE -> write DR -> wait RXNE -> read DR