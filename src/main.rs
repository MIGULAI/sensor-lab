#![no_std]
#![no_main]

mod bsp;
mod gpio;
mod i2c;
mod rcc;
mod spi;
mod state;
mod tasks;
mod screens;

use defmt_rtt as _;
use panic_probe as _;

use rtic_monotonics::systick::prelude::*;

// 1 kHz SysTick-based async timer (Mono::delay(...).await)
systick_monotonic!(Mono, 1_000);

#[rtic::app(device = stm32f4::stm32f411, peripherals = true, dispatchers = [EXTI0, EXTI1])]
mod app {
    use super::*;
    use crate::{
        bsp::{Initialized, led::Led},
        gpio::Port,
        rcc::{APB1_I2C1, APB1_TIM2, Rcc},
        state::State,
        // Task bodies live in `tasks/`; RTIC calls them by these names from inside `app`.
        tasks::{bmp280::bmp280_task, display::display_task, led::blink, sht40::sht40_task},
    };
    use rtic_sync::arbiter::Arbiter;
    use stm32f4::stm32f411 as pac;
    const LED_PIN: u8 = 13; // PC13, active-low

    #[shared]
    struct Shared {
        rcc: Rcc,
        state: State,
    }

    #[local]
    struct Local {
        led: Led<Initialized>, // PC13
        // SHT40 and BMP280 share I2C1; the Arbiter is an async mutex that can be held across `.await`.
        sht40_bus: &'static Arbiter<pac::I2C1>,
        bmp280_bus: &'static Arbiter<pac::I2C1>,
        display_bus: &'static Arbiter<pac::I2C1>,
        tim2: pac::TIM2, // free-running 1 MHz counter for CPU load measurement in idle
    }

    #[init(local = [i2c_bus: Option<Arbiter<pac::I2C1>> = None])]
    fn init(cx: init::Context) -> (Shared, Local) {
        let dp = cx.device;

        rcc::init(&dp.RCC, &dp.FLASH, &dp.PWR);
        Mono::start(cx.core.SYST, rcc::SYSCLK_HZ);

        gpio::enable_clocks();
        let led = Led::init(Port::C, LED_PIN, true);

        spi::init(&dp.RCC, &dp.SPI1);

        // Keeps the AHB bus matrix clocked during WFI, so the debugger's RAM reads (RTT) don't fail.
        dp.RCC.ahb1enr().modify(|_, w| w.dma1en().set_bit());

        let mut rcc = Rcc::new(dp.RCC);
        rcc.enable_reset_apb1(APB1_I2C1);
        i2c::init(&dp.I2C1);
        let i2c_bus: &'static Arbiter<pac::I2C1> = cx.local.i2c_bus.insert(Arbiter::new(dp.I2C1));

        // TIM2: 32-bit, keeps counting while the core sleeps in WFI (unlike the DWT cycle counter).
        rcc.enable_reset_apb1(APB1_TIM2);
        let tim2 = dp.TIM2;
        tim2.psc().write(|w| unsafe { w.bits(99) }); // 100 MHz timer clock (2 x APB1) / 100 = 1 MHz
        tim2.arr().write(|w| unsafe { w.bits(u32::MAX) }); // free-running over the full 32 bits
        tim2.egr().write(|w| w.ug().set_bit()); // load the prescaler now, not at the first overflow
        tim2.cr1().modify(|_, w| w.cen().set_bit());

        // Keep the debug connection (RTT/defmt) alive while the core sleeps in WFI.
        dp.DBGMCU.cr().modify(|_, w| w.dbg_sleep().set_bit());

        let state: State = State {
            bmp280_state: None,
            sht40_state: None,
        };
        defmt::info!("init done");
        blink::spawn().ok();
        sht40_task::spawn().ok();
        bmp280_task::spawn().ok();
        display_task::spawn().ok();

        (
            Shared { rcc, state },
            Local {
                led,
                sht40_bus: i2c_bus,
                bmp280_bus: i2c_bus,
                display_bus: i2c_bus,
                tim2,
            },
        )
    }

    // Runs whenever no task or interrupt is ready. CPU load = 1 - (time asleep / time elapsed).
    #[idle(local = [tim2])]
    fn idle(cx: idle::Context) -> ! {
        let tim = cx.local.tim2;
        let now = || tim.cnt().read().bits(); // µs, wraps every ~71 min; wrapping_sub handles it
        let mut window_start = now();
        let mut sleep_us: u32 = 0;

        loop {
            // With interrupts masked, WFI still wakes on a pending IRQ, but its handler runs only
            // after `enable()`, so the handler's time isn't counted as sleep.
            cortex_m::interrupt::disable();
            let t0 = now();
            cortex_m::asm::wfi();
            let t1 = now();
            unsafe { cortex_m::interrupt::enable() };
            sleep_us += t1.wrapping_sub(t0);

            let elapsed = t1.wrapping_sub(window_start);
            if elapsed >= 1_000_000 {
                let load = 100.0 * (1.0 - sleep_us as f32 / elapsed as f32);
                defmt::info!("CPU load: {=f32} %", load);
                window_start = t1;
                sleep_us = 0;
            }
        }
    }

    extern "Rust" {
        // priority = 1: tasks run from the EXTI0 dispatcher. The default (0) runs them in the
        // idle loop, which isn't allowed once a custom `#[idle]` exists.
        #[task(local = [led], priority = 1)]
        async fn blink(cx: blink::Context<'_>);

        #[task(local = [sht40_bus], shared = [state], priority = 1)]
        async fn sht40_task(cx: sht40_task::Context<'_>);

        #[task(local = [bmp280_bus], shared = [state], priority = 1)]
        async fn bmp280_task(cx: bmp280_task::Context<'_>);

        #[task(local = [display_bus], shared = [state], priority = 1)]
        async fn display_task(cx: display_task::Context<'_>);
    }
}
