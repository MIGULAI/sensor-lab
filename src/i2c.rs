//! I2C1 on PB6 (SCL) / PB7 (SDA), AF4, fast mode 400 kHz.
//!
//! Every failed transfer leaves the bus idle again:
//! - NACK is a normal answer (device absent or busy): the transfer is ended with STOP.
//! - Bus error, lost arbitration or timeout mean a glitch on SCL/SDA (e.g. a logic analyzer
//!   probe being attached) may have left a slave or the peripheral stuck, so the bus is
//!   recovered: SCL pulses free a slave holding SDA low, a STOP resets all slaves, and SWRST
//!   clears a BUSY flag latched by the glitch (errata ES0287 "I2C analog filter ... BUSY").
//!
//! Data bytes move by DMA (writes: DMA1 Stream 6, reads: Stream 0): the task sleeps until the
//! transfer ends, so the CPU is free (or in WFI) instead of polling for every byte. Only the
//! address phase, and 1-byte reads, are polled.
//!
//! A DMA transfer wakes up through "doorbell" interrupts (DMA1_STREAM6 / DMA1_STREAM0: transfer
//! ended, I2C1_ER: NACK / bus error / lost arbitration). They touch no I2C or DMA register: they
//! mask themselves and ring a signal. Only the holder of the bus lock reads and clears the flags,
//! so the lock really guards every I2C1 and DMA stream register the bus uses.

use crate::Mono;
use crate::dma::Dma1Stream;
use crate::gpio::{self, Mode, Port, Pull, Speed};
use crate::rcc::{APB1_HZ, SYSCLK_HZ};
use cortex_m::peripheral::NVIC;
use pac::Interrupt;
use rtic_monotonics::systick::prelude::*;
use rtic_sync::signal::{Signal, SignalReader};
use stm32f4::stm32f411 as pac;

const SCL: u8 = 6;
const SDA: u8 = 7;

/// SCL frequency. All devices on the bus support 400 kHz (SSD1306 400 kHz, SHT40 1 MHz, BMP280 3.4 MHz).
const BUS_HZ: u32 = 400_000;

#[derive(Clone, Copy, Debug, defmt::Format)]
pub enum I2CError {
    Nack,             // address or data byte not acknowledged
    Timeout,          // an expected flag never came: bus or peripheral stuck
    BusError,         // BERR: misplaced START/STOP seen on the bus, usually a glitch
    ArbitrationLost,  // ARLO: SDA read back differs from what we drove, usually a glitch
    UnexpectedDevice, // device answered but with the wrong chip ID
    InvalidData,      // CRC or other data check failed
    Dma,              // DMA transfer error (bad memory address)
}

const TIMEOUT: u32 = 100_000;

/// Error flags in SR1: bus error, lost arbitration, NACK.
fn check_errors(sr1: &pac::i2c1::sr1::R) -> Result<(), I2CError> {
    if sr1.berr().bit_is_set() {
        return Err(I2CError::BusError);
    }
    if sr1.arlo().bit_is_set() {
        return Err(I2CError::ArbitrationLost);
    }
    if sr1.af().bit_is_set() {
        return Err(I2CError::Nack);
    }
    Ok(())
}

/// Polls SR1 until `done`, failing early on NACK, bus error or lost arbitration.
fn wait(i2c: &pac::I2C1, done: impl Fn(&pac::i2c1::sr1::R) -> bool) -> Result<(), I2CError> {
    for _ in 0..TIMEOUT {
        let sr1 = i2c.sr1().read();
        check_errors(&sr1)?;
        if done(&sr1) {
            return Ok(());
        }
    }
    Err(I2CError::Timeout)
}

fn wait_idle(i2c: &pac::I2C1) -> Result<(), I2CError> {
    for _ in 0..TIMEOUT {
        if i2c.sr2().read().busy().bit_is_clear() {
            return Ok(());
        }
    }
    Err(I2CError::Timeout)
}

/// I2C1 together with the DMA streams it uses. The bus Arbiter owns this, so whoever holds the
/// bus lock owns all of them; no other code can reach any. Methods take `&mut self`, which the
/// Arbiter guard provides, so overlapping transfers are impossible by construction.
pub struct I2cBus {
    i2c: pac::I2C1,
    tx: TxStream,
    rx: RxStream,
}

impl I2cBus {
    /// Takes I2C1 and its TX and RX DMA streams (I2C1 and DMA1 clocks must already be enabled)
    /// and configures I2C1.
    pub fn new(i2c: pac::I2C1, tx: TxStream, rx: RxStream) -> Self {
        configure_pins();
        configure_peripheral(&i2c);
        Self { i2c, tx, rx }
    }

    /// Writes all `bytes` in one transaction (e.g. `[reg, value]` for a register write).
    pub async fn write(&mut self, addr: u8, bytes: &[u8]) -> Result<(), I2CError> {
        let result = write_dma(&self.i2c, &self.tx, addr, bytes).await;
        if let Err(e) = result {
            cleanup(&self.i2c, e);
        }
        result
    }

    pub async fn write_byte(&mut self, addr: u8, byte: u8) -> Result<(), I2CError> {
        self.write(addr, &[byte]).await
    }

    /// Reads `buf.len()` bytes in one transaction.
    pub async fn read(&mut self, addr: u8, buf: &mut [u8]) -> Result<(), I2CError> {
        let result = read_dma(&self.i2c, &self.rx, addr, buf).await;
        if let Err(e) = result {
            cleanup(&self.i2c, e);
        }
        result
    }
}

fn configure_pins() {
    for pin in [SCL, SDA] {
        gpio::set_alternate(Port::B, pin, 4);
        gpio::configure(Port::B, pin, Mode::Alternate, Pull::Up, Speed::High, true);
    }
}

fn configure_peripheral(i2c: &pac::I2C1) {
    let freq_mhz = APB1_HZ / 1_000_000;
    i2c.cr1().modify(|_, w| w.pe().clear_bit());
    i2c.cr2()
        .modify(|_, w| unsafe { w.freq().bits(freq_mhz as u8) });
    // Error flags always raise I2C1_ER; whether anyone listens is decided by its NVIC mask, which
    // is open only during the DMA phase of a transfer (see `arm_doorbells`).
    i2c.cr2().modify(|_, w| w.iterren().set_bit());
    // Fast mode, DUTY = 0: T_low = 2 * T_high, period = 3 * CCR * T_pclk -> CCR = PCLK / (3 * f_scl).
    // Rounded up so SCL stays <= 400 kHz: 50 MHz -> CCR 42 -> 397 kHz (T_low 1.68 us >= 1.3 us spec).
    let ccr = APB1_HZ.div_ceil(3 * BUS_HZ) as u16;
    i2c.ccr()
        .write(|w| unsafe { w.f_s().set_bit().duty().clear_bit().ccr().bits(ccr) });
    // Fast-mode max rise time 300 ns -> TRISE = 0.3 * PCLK(MHz) + 1.
    i2c.trise()
        .write(|w| unsafe { w.trise().bits((freq_mhz * 3 / 10 + 1) as u8) });
    i2c.cr1().modify(|_, w| w.pe().set_bit());
}

/// Puts the bus back in a usable state after a failed transfer.
fn cleanup(i2c: &pac::I2C1, err: I2CError) {
    match err {
        I2CError::Nack => {
            i2c.sr1().modify(|_, w| w.af().clear_bit());
            i2c.cr1().modify(|_, w| w.stop().set_bit().ack().set_bit());
        }
        _ => {
            defmt::warn!("I2C: {:?}, recovering bus", err);
            recover(i2c);
        }
    }
}

/// 5 µs busy-wait: half a 100 kHz clock period, used while bit-banging the bus.
fn half_bit() {
    cortex_m::asm::delay(SYSCLK_HZ / 200_000);
}

/// Frees a stuck bus and resets the peripheral (RM0383 I2C SWRST + ES0287 BUSY workaround).
fn recover(i2c: &pac::I2C1) {
    i2c.cr1().modify(|_, w| w.pe().clear_bit());

    // Take both lines over as open-drain GPIO, released (high) before switching mode.
    for pin in [SCL, SDA] {
        gpio::set_high(Port::B, pin);
        gpio::configure(Port::B, pin, Mode::Output, Pull::Up, Speed::High, true);
    }

    // A slave cut off mid-byte keeps driving SDA low, waiting for clocks: give it up to 9.
    for _ in 0..9 {
        if gpio::is_high(Port::B, SDA) {
            break;
        }
        gpio::set_low(Port::B, SCL);
        half_bit();
        gpio::set_high(Port::B, SCL);
        half_bit();
    }

    // STOP (SDA rises while SCL is high) resets every slave's state machine.
    gpio::set_low(Port::B, SCL);
    half_bit();
    gpio::set_low(Port::B, SDA);
    half_bit();
    gpio::set_high(Port::B, SCL);
    half_bit();
    gpio::set_high(Port::B, SDA);
    half_bit();

    if !gpio::is_high(Port::B, SDA) || !gpio::is_high(Port::B, SCL) {
        defmt::error!("I2C: bus still held low after recovery");
    }

    // SWRST clears BUSY and error flags latched by the glitch, but also wipes the timing config.
    configure_pins();
    i2c.cr1().modify(|_, w| w.swrst().set_bit());
    i2c.cr1().modify(|_, w| w.swrst().clear_bit());
    configure_peripheral(i2c);
}

fn start(i2c: &pac::I2C1, addr: u8, read: bool) -> Result<(), I2CError> {
    wait_idle(i2c)?;
    i2c.cr1().modify(|_, w| w.start().set_bit());
    wait(i2c, |sr1| sr1.sb().bit_is_set())?;
    i2c.dr().write(|w| w.dr().set((addr << 1) | read as u8));
    wait(i2c, |sr1| sr1.addr().bit_is_set())
}

// ---- DMA data phase ----------------------------------------------------------------------------

/// I2C1_TX = DMA1 Stream 6 and I2C1_RX = DMA1 Stream 0, both on channel 1
/// (RM0383 table 27 "DMA1 request mapping").
pub type TxStream = Dma1Stream<6>;
pub type RxStream = Dma1Stream<0>;
const DMA_CHANNEL: u8 = 1;

/// Rung by any doorbell interrupt; awaited in `dma_transfer`. Carries no data: the woken task
/// reads the flags itself.
static BUS_EVENT: Signal<()> = Signal::new();

/// The interrupts that can end a DMA transfer: TX or RX stream complete/error, and I2C errors.
const DOORBELLS: [Interrupt; 3] = [
    Interrupt::DMA1_STREAM6,
    Interrupt::DMA1_STREAM0,
    Interrupt::I2C1_ER,
];

/// Body of the DMA1_STREAM6 interrupt (bound in main.rs).
pub fn on_tx_dma_irq() {
    ring(Interrupt::DMA1_STREAM6);
}

/// Body of the DMA1_STREAM0 interrupt (bound in main.rs).
pub fn on_rx_dma_irq() {
    ring(Interrupt::DMA1_STREAM0);
}

/// Body of the I2C1_ER interrupt (bound in main.rs).
pub fn on_error_irq() {
    ring(Interrupt::I2C1_ER);
}

/// Doorbell: touches no I2C1 or DMA register, so the bus lock stays the only way to reach them.
/// The flag that raised the interrupt is still set (it's cleared by the lock holder), so the
/// interrupt masks itself to stop re-firing, then wakes the task.
fn ring(irq: Interrupt) {
    NVIC::mask(irq);
    BUS_EVENT.writer().write(());
}

/// Opens the doorbells for the DMA phase of one transfer.
fn arm_doorbells(event: &mut SignalReader<'_, ()>) {
    let _ = event.try_read(); // a ring left over from an earlier transfer
    for irq in DOORBELLS {
        // Latched while masked (e.g. a NACK seen by the polled address phase of an earlier
        // transfer): would ring right away.
        NVIC::unpend(irq);
        // SAFETY: RTIC on ARMv7-M implements its locks with BASEPRI, not NVIC masks, so
        // unmasking these can't break a critical section.
        unsafe { NVIC::unmask(irq) };
    }
}

fn disarm_doorbells() {
    for irq in DOORBELLS {
        NVIC::mask(irq);
    }
}

/// Stops a DMA stream and detaches it from I2C1 when dropped. The DMA reads or writes the
/// caller's buffer behind the borrow checker's back, so it must be stopped before that borrow
/// ends, also when the transfer future is dropped mid-transfer. For RX that matters most: a
/// runaway DMA would write into memory that is already used for something else.
struct DmaGuard<'a, const N: usize> {
    i2c: &'a pac::I2C1,
    stream: &'a Dma1Stream<N>,
}

impl<const N: usize> Drop for DmaGuard<'_, N> {
    fn drop(&mut self) {
        disarm_doorbells();
        let regs = self.stream.regs();
        regs.cr().modify(|_, w| w.en().disabled());
        while regs.cr().read().en().bit_is_set() {} // ends after the current single transfer
        self.i2c.cr2().modify(|_, w| w.dmaen().clear_bit().last().clear_bit());
        self.stream.clear_flags();
    }
}

enum Direction {
    ToBus,   // write: memory -> DR
    FromBus, // read: DR -> memory
}

/// DMA transfer length: NDTR is 16 bits and 0 is not a valid length.
fn dma_len(n: usize) -> Result<u16, I2CError> {
    match u16::try_from(n) {
        Ok(len) if len > 0 => Ok(len),
        _ => Err(I2CError::InvalidData),
    }
}

/// Data phase of a transfer by DMA. Call right after `start` (ADDR set, not cleared yet).
///
/// `mem` is the address of the caller's buffer, which the caller keeps borrowed across this call.
/// The guard stops the DMA before this returns, also when this future is dropped, so the DMA
/// never outlives that borrow.
async fn dma_transfer<const N: usize>(
    i2c: &pac::I2C1,
    stream: &Dma1Stream<N>,
    mem: u32,
    len: u16,
    direction: Direction,
) -> Result<(), I2CError> {
    let mut event = BUS_EVENT.take_reader();

    let regs = stream.regs();
    stream.clear_flags();
    regs.par().write(|w| unsafe { w.pa().bits(i2c.dr().as_ptr() as u32) });
    regs.m0ar().write(|w| unsafe { w.m0a().bits(mem) });
    regs.ndtr().write(|w| w.ndt().set(len));
    regs.cr().write(|w| {
        w.chsel().set(DMA_CHANNEL);
        match direction {
            Direction::ToBus => w.dir().memory_to_peripheral(),
            Direction::FromBus => w.dir().peripheral_to_memory(),
        };
        w.minc().incremented(); // walk through the buffer, DR address stays fixed
        w.psize().bits8();
        w.msize().bits8();
        w.tcie().enabled();
        w.teie().enabled();
        w.en().enabled()
    });
    let _guard = DmaGuard { i2c, stream };

    arm_doorbells(&mut event);
    i2c.cr2().modify(|_, w| w.dmaen().set_bit());
    let _ = i2c.sr2().read(); // clear ADDR: the data phase starts, the DMA moves every byte

    // Sleep until a doorbell rings (DMA ended, or an I2C error stopped it) or, if nothing happens
    // at all (e.g. SCL held low), the timeout: 9 bits per byte on the wire plus margin.
    let timeout_ms = len as u32 * 9 * 1000 / BUS_HZ + 5;
    let _ = Mono::timeout_after(timeout_ms.millis(), event.wait()).await;
    disarm_doorbells();

    // Whatever woke us, the flags tell what happened.
    check_errors(&i2c.sr1().read())?;
    let flags = stream.flags();
    if flags.transfer_error() {
        return Err(I2CError::Dma);
    }
    if !flags.transfer_complete() {
        return Err(I2CError::Timeout);
    }
    Ok(())
}

// ---- Write -------------------------------------------------------------------------------------

async fn write_dma(
    i2c: &pac::I2C1,
    tx: &TxStream,
    addr: u8,
    bytes: &[u8],
) -> Result<(), I2CError> {
    let len = dma_len(bytes.len())?;

    // Address phase stays polled: one byte, ~25 us at 400 kHz.
    start(i2c, addr, false)?;
    dma_transfer(i2c, tx, bytes.as_ptr() as u32, len, Direction::ToBus).await?;

    // DMA complete means the last byte reached DR, not the wire: wait for BTF before STOP. A NACK
    // of that last byte comes only now, and `wait` catches it in AF.
    wait(i2c, |sr1| sr1.btf().bit_is_set())?;
    i2c.cr1().modify(|_, w| w.stop().set_bit());
    Ok(())
}

// ---- Read --------------------------------------------------------------------------------------

async fn read_dma(
    i2c: &pac::I2C1,
    rx: &RxStream,
    addr: u8,
    buf: &mut [u8],
) -> Result<(), I2CError> {
    // A single byte stays polled: its NACK and STOP must be set right at ADDR (RM0383 27.3.3),
    // and the whole read takes ~25 us.
    if buf.len() == 1 {
        buf[0] = read_byte(i2c, addr)?;
        return Ok(());
    }
    let len = dma_len(buf.len())?;

    i2c.cr1().modify(|_, w| w.ack().set_bit()); // ACK every byte...
    start(i2c, addr, true)?;
    // ...except the last: LAST makes the hardware NACK it when the DMA reaches its end
    // (RM0383 27.3.7). This replaces the N-2/N-1/N sequence of polled reads. The guard in
    // `dma_transfer` clears LAST again.
    i2c.cr2().modify(|_, w| w.last().set_bit());
    dma_transfer(i2c, rx, buf.as_mut_ptr() as u32, len, Direction::FromBus).await?;

    // All bytes received, the last one NACKed: end the transaction.
    i2c.cr1().modify(|_, w| w.stop().set_bit());
    Ok(())
}

fn read_byte(i2c: &pac::I2C1, addr: u8) -> Result<u8, I2CError> {
    start(i2c, addr, true)?;
    i2c.cr1().modify(|_, w| w.ack().clear_bit()); // 1) NACK the only byte
    let _ = i2c.sr2().read(); //                     2) clear ADDR: reception starts
    i2c.cr1().modify(|_, w| w.stop().set_bit()); //  3) STOP after this byte
    wait(i2c, |sr1| sr1.rx_ne().bit_is_set())?;
    let byte = i2c.dr().read().dr().bits();
    i2c.cr1().modify(|_, w| w.ack().set_bit()); //   restore ACK for next reads
    Ok(byte)
}
