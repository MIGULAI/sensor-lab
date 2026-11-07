use crate::{Mono, i2c};
use rtic_monotonics::systick::prelude::*;
use rtic_sync::arbiter::Arbiter;
use crate::i2c::I2cBus;

pub mod bmp280;
pub mod led;
pub mod sht40;
pub mod ssd1306_display;
pub struct Uninitialized;
pub struct Initialized;

#[derive(Clone)]
pub struct SensorState<T, C> {
    pub measurements: Option<T>,
    pub last_measurement_time: Option<u32>,
    pub id: Option<u32>,
    pub healthy: bool,
    pub error: bool,
    pub config: Option<C>,
}

pub struct I2CPeripheral {
    pub bus: &'static Arbiter<I2cBus>,
    pub addr: u8,
}

impl I2CPeripheral {
    pub fn new(bus: &'static Arbiter<I2cBus>, addr: u8) -> Self {
        I2CPeripheral { bus, addr }
    }
    async fn call<const N: usize>(
        &self,
        command: u8,
        delay_ms: u32,
    ) -> Result<[u8; N], i2c::I2CError> {
        let result = self.bus.access().await.write_byte(self.addr, command).await;
        if let Err(e) = result {
            defmt::error!("I2C {=u8:#x}: write error: {:?}", self.addr, e);
            return Err(e);
        } else {
            Mono::delay(delay_ms.millis()).await;
            let mut buf = [0u8; N];
            match self.bus.access().await.read(self.addr, &mut buf).await {
                Ok(()) => {
                    return Ok(buf);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    async fn command(&self, command: u8, delay_ms: u32) -> Result<(), i2c::I2CError> {
        let result = self.bus.access().await.write_byte(self.addr, command).await;
        if let Err(e) = result {
            defmt::error!("I2C {=u8:#x}: write error: {:?}", self.addr, e);
            return Err(e);
        } else {
            Mono::delay(delay_ms.millis()).await;
            return Ok(());
        }
    }

    async fn write(&self, data: &[u8]) -> Result<(), i2c::I2CError> {
        if data.is_empty() {
            return Err(i2c::I2CError::InvalidData);
        }
        let result = self.bus.access().await.write(self.addr, data).await;
        if let Err(e) = result {
            defmt::error!("I2C {=u8:#x}: write error: {:?}", self.addr, e);
            return Err(e);
        } else {
            return Ok(());
        }
    }
}
