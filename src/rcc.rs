#![allow(dead_code)] // unused variants/constants kept for future use

//! Clock tree: 25 MHz HSE -> PLL -> 100 MHz SYSCLK.
//! AHB = 100 MHz, APB1 = 50 MHz (max), APB2 = 100 MHz.

use stm32f4::stm32f411 as pac;

pub const RCC_BASE: u32 = 0x4002_3800;

pub const RCC_APB1ENR_REG: *mut u32 = (RCC_BASE + 0x40) as *mut u32;
pub const RCC_APB1RSTR_REG: *mut u32 = (RCC_BASE + 0x20) as *mut u32;

pub const SYSCLK_HZ: u32 = 100_000_000;
pub const APB1_HZ: u32 = 50_000_000;
pub const APB2_HZ: u32 = 100_000_000;

pub const APB1_TIM2: u32 = 0;
pub const APB1_I2C1: u32 = 21;
pub const APB2_SPI1: u32 = 12;

pub struct Rcc {
    _private: (),
}

impl Rcc {
    pub fn new(_rcc: pac::RCC) -> Self {
        Self { _private: () }
    }
    pub fn enable_reset_apb1(&mut self, bit: u32) {
        unsafe {
            let v = core::ptr::read_volatile(RCC_APB1ENR_REG);
            core::ptr::write_volatile(RCC_APB1ENR_REG, v | (1 << bit));
            let v = core::ptr::read_volatile(RCC_APB1RSTR_REG);
            core::ptr::write_volatile(RCC_APB1RSTR_REG, v | (1 << bit));
            core::ptr::write_volatile(RCC_APB1RSTR_REG, v & !(1 << bit));
        }
    }
}

pub fn init(rcc: &pac::RCC, flash: &pac::FLASH, pwr: &pac::PWR) {
    // Voltage scale 1 is required for 100 MHz.
    unsafe {
        let v = core::ptr::read_volatile(RCC_APB1ENR_REG);
        core::ptr::write_volatile(RCC_APB1ENR_REG, v | (1 << 28));
    }
    pwr.cr().modify(|_, w| unsafe { w.vos().bits(0b11) });

    // Start HSE and wait until stable.
    rcc.cr().modify(|_, w| w.hseon().set_bit());
    while rcc.cr().read().hserdy().bit_is_clear() {}

    // PLL: 25 MHz / M(25) = 1 MHz, * N(200) = 200 MHz VCO, / P(2) = 100 MHz.
    rcc.pllcfgr().write(|w| unsafe {
        w.pllsrc()
            .set_bit() // HSE
            .pllm()
            .bits(25)
            .plln()
            .bits(200)
            .pllp()
            .bits(0b00) // /2
            .pllq()
            .bits(4)
    });
    rcc.cr().modify(|_, w| w.pllon().set_bit());
    while rcc.cr().read().pllrdy().bit_is_clear() {}

    // 3 wait states for 90..100 MHz @ 3.3 V, plus prefetch and caches.
    flash.acr().modify(|_, w| unsafe {
        w.latency()
            .bits(3)
            .prften()
            .set_bit()
            .icen()
            .set_bit()
            .dcen()
            .set_bit()
    });
    while flash.acr().read().latency().bits() != 3 {}

    // AHB /1, APB1 /2, APB2 /1, then switch SYSCLK to PLL.
    rcc.cfgr().modify(|_, w| unsafe {
        w.hpre()
            .bits(0b0000)
            .ppre1()
            .bits(0b100)
            .ppre2()
            .bits(0b000)
    });
    rcc.cfgr().modify(|_, w| unsafe { w.sw().bits(0b10) });
    while rcc.cfgr().read().sws().bits() != 0b10 {}
}
