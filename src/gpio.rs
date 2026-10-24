#![allow(dead_code)] // unused variants/constants kept for future use

//! Minimal GPIO configuration. All STM32F4 GPIO ports share one register
//! layout, so every port is accessed through the GPIOA register block type.

use stm32f4::stm32f411 as pac;

use crate::rcc::RCC_BASE;

type Regs = pac::gpioa::RegisterBlock;

pub const RCC_AHB1ENR_REG: *mut u32 = (RCC_BASE + 0x30) as *mut u32;

#[derive(Clone, Copy)]
pub enum Port {
    A,
    B,
    C,
}

impl Port {
    fn regs(self) -> &'static Regs {
        let ptr = match self {
            Port::A => pac::GPIOA::ptr(),
            Port::B => pac::GPIOB::ptr() as *const Regs,
            Port::C => pac::GPIOC::ptr() as *const Regs,
        };
        unsafe { &*ptr }
    }
}

#[derive(Clone, Copy)]
pub enum Mode {
    Input = 0b00,
    Output = 0b01,
    Alternate = 0b10,
    Analog = 0b11,
}

#[derive(Clone, Copy)]
pub enum Pull {
    None = 0b00,
    Up = 0b01,
    Down = 0b10,
}

#[derive(Clone, Copy)]
pub enum Speed {
    Low = 0b00,
    Medium = 0b01,
    High = 0b10,
    VeryHigh = 0b11,
}

pub fn enable_clocks() {
    unsafe {
        let v = core::ptr::read_volatile(RCC_AHB1ENR_REG);
        core::ptr::write_volatile(RCC_AHB1ENR_REG, v | (1 << 0) | (1 << 1) | (1 << 2));
    }
}

/// Read-modify-write of a `width`-bit field for `pin` in a register.
macro_rules! set_field {
    ($reg:expr, $pin:expr, $width:expr, $val:expr) => {{
        let shift = $pin as u32 * $width;
        let mask = ((1u32 << $width) - 1) << shift;
        $reg.modify(|r, w| unsafe { w.bits((r.bits() & !mask) | (($val as u32) << shift)) });
    }};
}

pub fn configure(port: Port, pin: u8, mode: Mode, pull: Pull, speed: Speed, open_drain: bool) {
    let g = port.regs();
    set_field!(g.moder(), pin, 2, mode);
    set_field!(g.pupdr(), pin, 2, pull);
    set_field!(g.ospeedr(), pin, 2, speed);
    set_field!(g.otyper(), pin, 1, open_drain);
}

pub fn set_alternate(port: Port, pin: u8, af: u8) {
    let g = port.regs();
    if pin < 8 {
        set_field!(g.afrl(), pin, 4, af);
    } else {
        set_field!(g.afrh(), pin - 8, 4, af);
    }
}

pub fn set_high(port: Port, pin: u8) {
    port.regs().bsrr().write(|w| unsafe { w.bits(1 << pin) });
}

pub fn set_low(port: Port, pin: u8) {
    port.regs().bsrr().write(|w| unsafe { w.bits(1 << (pin + 16)) });
}

pub fn toggle(port: Port, pin: u8) {
    let g = port.regs();
    if g.odr().read().bits() & (1 << pin) != 0 {
        set_low(port, pin)
    } else {
        set_high(port, pin)
    }
}