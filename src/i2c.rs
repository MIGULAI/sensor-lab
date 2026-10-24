//! I2C1 on PB6 (SCL) / PB7 (SDA), AF4, standard mode 100 kHz.

use crate::gpio::{self, Mode, Port, Pull, Speed};
use crate::rcc::APB1_HZ;
use stm32f4::stm32f411 as pac;

#[derive(Debug, defmt::Format)]
pub enum I2CError {
    Nack,
    Timeout,
    UnexpectedDevice,
}

const TIMEOUT: u32 = 100_000;

fn wait(mut done: impl FnMut() -> bool) -> Result<(), I2CError> {
    for _ in 0..TIMEOUT {
        if done() {
            return Ok(());
        }
    }
    Err(I2CError::Timeout)
}

pub fn init(i2c: &pac::I2C1) {
    for pin in [6, 7] {
        gpio::set_alternate(Port::B, pin, 4);
        gpio::configure(Port::B, pin, Mode::Alternate, Pull::Up, Speed::High, true);
    }

    let freq_mhz = APB1_HZ / 1_000_000;
    i2c.cr1().modify(|_, w| w.pe().clear_bit());
    i2c.cr2()
        .modify(|_, w| unsafe { w.freq().bits(freq_mhz as u8) });
    // Standard mode: T_high = T_low = CCR * T_pclk -> CCR = PCLK / (2 * 100 kHz).
    i2c.ccr()
        .write(|w| unsafe { w.f_s().clear_bit().ccr().bits((APB1_HZ / 200_000) as u16) });
    // Max rise time 1000 ns -> TRISE = PCLK(MHz) + 1.
    i2c.trise()
        .write(|w| unsafe { w.trise().bits(freq_mhz as u8 + 1) });
    i2c.cr1().modify(|_, w| w.pe().set_bit());
}


fn start(i2c: &pac::I2C1, addr: u8, read: bool) -> Result<(), I2CError> {
    wait(|| i2c.sr2().read().busy().bit_is_clear())?;
    i2c.cr1().modify(|_, w| w.start().set_bit());
    wait(|| i2c.sr1().read().sb().bit_is_set())?;
    i2c.dr().write(|w| w.dr().set((addr << 1) | read as u8));

    for _ in 0..TIMEOUT {
        let sr1 = i2c.sr1().read();
        if sr1.af().bit_is_set() {
            i2c.sr1().write(|w| w.af().clear_bit());
            i2c.cr1().modify(|_, w| w.stop().set_bit());
            return Err(I2CError::Nack);
        }
        if sr1.addr().bit_is_set() {
            return Ok(());
        }
    }
    Err(I2CError::Timeout)
}

pub fn write_byte(i2c: &pac::I2C1, addr: u8, byte: u8) -> Result<(), I2CError> {
    write(i2c, addr, &[byte])
}

// Writes all `bytes` in one transaction (e.g. `[reg, value]` for a register write).
pub fn write(i2c: &pac::I2C1, addr: u8, bytes: &[u8]) -> Result<(), I2CError> {
    start(i2c, addr, false)?;
    let _ = i2c.sr2().read(); // clear ADDR

    for &byte in bytes {
        wait(|| i2c.sr1().read().tx_e().bit_is_set())?;
        i2c.dr().write(|w| w.dr().set(byte));
        wait(|| i2c.sr1().read().btf().bit_is_set())?; // byte fully on the wire
        if i2c.sr1().read().af().bit_is_set() {
            i2c.sr1().write(|w| w.af().clear_bit());
            i2c.cr1().modify(|_, w| w.stop().set_bit());
            return Err(I2CError::Nack);
        }
    }
    i2c.cr1().modify(|_, w| w.stop().set_bit());
    Ok(())
}
pub fn read_byte(i2c: &pac::I2C1, addr: u8) -> Result<u8, I2CError> {
    start(i2c, addr, true)?;
    i2c.cr1().modify(|_, w| w.ack().clear_bit()); // 1) NACK the only byte
    let _ = i2c.sr2().read(); //                     2) clear ADDR: reception starts
    i2c.cr1().modify(|_, w| w.stop().set_bit()); //  3) STOP after this byte
    wait(|| i2c.sr1().read().rx_ne().bit_is_set())?;
    let byte = i2c.dr().read().dr().bits();
    i2c.cr1().modify(|_, w| w.ack().set_bit()); //   restore ACK for next reads
    Ok(byte)
}

pub fn read(i2c: &pac::I2C1, addr: u8, buf: &mut [u8]) -> Result<(), I2CError> {
    let n = buf.len();
    if n == 0 {
        return Ok(());
    }
    if n == 1 {
        buf[0] = read_byte(i2c, addr)?;
        return Ok(());
    }
    assert!(n >= 3, "2-byte reads need the POS sequence (RM0383 27.3.3), not implemented");

    i2c.cr1().modify(|_, w| w.ack().set_bit()); // ACK every byte except the last
    start(i2c, addr, true)?;
    let _ = i2c.sr2().read(); // clear ADDR: reception starts

    // All bytes except the last three: plain RXNE polling.
    for b in &mut buf[..n - 3] {
        wait(|| i2c.sr1().read().rx_ne().bit_is_set())?;
        *b = i2c.dr().read().dr().bits();
    }

    // Byte N-2 in DR, byte N-1 in shift register, SCL held low.
    wait(|| i2c.sr1().read().btf().bit_is_set())?;
    i2c.cr1().modify(|_, w| w.ack().clear_bit()); // last byte will get NACK
    buf[n - 3] = i2c.dr().read().dr().bits();

    // Byte N-1 in DR, byte N in shift register.
    wait(|| i2c.sr1().read().btf().bit_is_set())?;
    i2c.cr1().modify(|_, w| w.stop().set_bit());
    buf[n - 2] = i2c.dr().read().dr().bits();

    wait(|| i2c.sr1().read().rx_ne().bit_is_set())?;
    buf[n - 1] = i2c.dr().read().dr().bits();

    i2c.cr1().modify(|_, w| w.ack().set_bit());
    Ok(())
}
