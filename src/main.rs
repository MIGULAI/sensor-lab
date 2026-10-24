#![no_std]
#![no_main]

mod bsp;
mod gpio;
mod i2c;
mod rcc;
mod spi;

use defmt_rtt as _;
use panic_probe as _;

use rtic_monotonics::systick::prelude::*;

// 1 kHz SysTick-based async timer (Mono::delay(...).await)
systick_monotonic!(Mono, 1_000);

#[rtic::app(device = stm32f4::stm32f411, peripherals = true, dispatchers = [EXTI0, EXTI1])]
mod app {
    use super::*;
    use crate::{
        bsp::{Initialized, bmp280::{MeasurementConfig, driver::BMP280}, led::Led, sht40::{Precision, driver::SHT40, measurement}},
        gpio::Port, rcc::{APB1_I2C1, Rcc},
    };
    use rtic_sync::arbiter::Arbiter;
    use stm32f4::stm32f411 as pac;
    const LED_PIN: u8 = 13; // PC13, active-low

    #[shared]
    struct Shared {
        rcc: Rcc,
    }

    #[local]
    struct Local {
        led: Led<Initialized>, // PC13
        // SHT40 and BMP280 share I2C1; the Arbiter is an async mutex that can be held across `.await`.
        sht40_bus: &'static Arbiter<pac::I2C1>,
        bmp280_bus: &'static Arbiter<pac::I2C1>,
    }

    #[init(local = [i2c_bus: Option<Arbiter<pac::I2C1>> = None])]
    fn init(cx: init::Context) -> (Shared, Local) {
        let dp = cx.device;

        rcc::init(&dp.RCC, &dp.FLASH, &dp.PWR);
        Mono::start(cx.core.SYST, rcc::SYSCLK_HZ);

        gpio::enable_clocks();
        let led = Led::init(Port::C, LED_PIN, true);

        spi::init(&dp.RCC, &dp.SPI1);

        let mut rcc = Rcc::new(dp.RCC);
        rcc.enable_reset_apb1(APB1_I2C1);
        i2c::init(&dp.I2C1);
        let i2c_bus: &'static Arbiter<pac::I2C1> = cx.local.i2c_bus.insert(Arbiter::new(dp.I2C1));

        defmt::info!("init done");
        blink::spawn().ok();
        sht40_task::spawn().ok();
        bmp280_task::spawn().ok();

        (
            Shared { rcc },
            Local {
                led,
                sht40_bus: i2c_bus,
                bmp280_bus: i2c_bus,
            },
        )
    }

    #[task(local = [led])]
    async fn blink(cx: blink::Context) {
        loop {
            cx.local.led.toggle();
            Mono::delay(500.millis()).await;
        }
    }

    #[task(local = [sht40_bus])]
    async fn sht40_task(cx: sht40_task::Context) {
        let sht40 = match SHT40::new(*cx.local.sht40_bus).init().await {
            Ok(sht40) => sht40,
            Err(e) => {
                defmt::error!("SHT40: failed to initialize: {:?}", e);
                return;
            }
        };
        defmt::info!("SHT40: serial number: {=u32:#x}", sht40.serial);
        loop {
            let measurement = sht40.get_temperature_humidity(Precision::High).await;
            if let Ok(measurement) = measurement {
                defmt::info!("SHT40: temp {=f32}, hum {=f32}", measurement.temperature_celsius(), measurement.humidity_percent());
            } else {
                defmt::error!("SHT40: failed to get measurement: {:?}", measurement.err().unwrap());
            }
            Mono::delay(1000.millis()).await;
        }
    }

    #[task(local = [bmp280_bus], shared = [rcc])]
    async fn bmp280_task(cx: bmp280_task::Context) {
        let bus = *cx.local.bmp280_bus;
        let bmp280 = match BMP280::new(bus).init().await {
            Ok(bmp280) => bmp280,
            Err(e) => {
                defmt::error!("BMP280: failed to initialize: {:?}", e);
                return;
            }
        };
        defmt::info!("BMP280: chip ID: {=u8:#x}", bmp280.chip_id);
        let config = MeasurementConfig::weather_monitoring();
        bmp280.set_configuration(config).await.ok();
        defmt::info!("BMP280: set configuration: {:?}", config);
        loop {
            bmp280.start_measurement(config).await.ok();
            let measurement = bmp280.get_temperature_pressure().await;
            if let Ok((temp, pressure)) = measurement {
                defmt::info!("BMP280: temp {=f32}, pressure {=f32}", temp, pressure);
            } else {
                defmt::error!("BMP280: failed to get measurement: {:?}", measurement.err().unwrap());
            }
            Mono::delay(1000.millis()).await;
        }
    }
}
