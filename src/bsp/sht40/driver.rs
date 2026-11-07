use core::marker::PhantomData;
use rtic_sync::arbiter::Arbiter;
use crate::i2c::I2cBus;

use crate::{
     bsp::{
        I2CPeripheral, Initialized, Uninitialized, sht40::{HeaterMode, Precision, measurement::SHT40Measurement},
    }, i2c,
};

pub const SHT40_I2C_ADDR: u8 = 0x44;

pub struct SHT40<State> {
    inner: I2CPeripheral,
    _state: PhantomData<State>,
    pub serial: u32,
}


impl SHT40<Uninitialized> {
    async fn get_serial(&self) -> Result<u32, i2c::I2CError> {
        self.inner.call::<6>(0x89, 10).await.map(|buf| {
            let high = u16::from_be_bytes([buf[0], buf[1]]); // buf[2] = CRC
            let low = u16::from_be_bytes([buf[3], buf[4]]); // buf[5] = CRC
            let serial = ((high as u32) << 16) | low as u32;
            serial
        })
    }
    pub fn new(bus: &'static Arbiter<I2cBus>) -> Self {
        SHT40 {
            _state: PhantomData::<Uninitialized>,
            inner: I2CPeripheral::new(bus, SHT40_I2C_ADDR),
            serial: 0,
        }
    }

    // Consumes the uninitialized driver and returns the initialized one.
    pub async fn init(self) -> Result<SHT40<Initialized>, i2c::I2CError> {
        let serial = match self.get_serial().await {
            Ok(serial) => serial,
            Err(e) => {
                defmt::error!("SHT40: failed to get serial number: {:?}", e);
                return Err(e);
            }
        };
        Ok(SHT40 {
            _state: PhantomData::<Initialized>,
            inner: self.inner,
            serial,
        })
    }
}

impl SHT40<Initialized> {
    // Get a temperature and humidity measurement from the SHT40 sensor.
    // The measurement is returned as raw 16-bit values, which can be converted to physical units using the formulas in the SHT40 datasheet.
    pub async fn get_temperature_humidity(
        &self,
        precision: Precision,
    ) -> Result<SHT40Measurement, i2c::I2CError> {
        let command = match precision {
            Precision::High => 0xFD,
            Precision::Medium => 0xF6,
            Precision::Low => 0xE0,
        };
        let buf = self.inner.call::<6>(command, 10).await;
        match buf {
            Ok(buf) => {
                let measurement = {
                    let temp_raw = u16::from_be_bytes([buf[0], buf[1]]); // buf[2] = CRC
                    let hum_raw = u16::from_be_bytes([buf[3], buf[4]]); // buf[5] = CRC
                    SHT40Measurement {
                        temperature: temp_raw,
                        humidity: hum_raw,
                    }
                };
                Ok(measurement)
            }
            Err(e) => Err(e),
        }
    }

    #[allow(dead_code)]
    pub async fn activate_heater(&self, mode: HeaterMode) -> Result<SHT40Measurement, i2c::I2CError> {
        let command = match mode {
            HeaterMode::High_1s => 0x39,
            HeaterMode::High_0_1s => 0x32,
            HeaterMode::Medium_1s => 0x2F,
            HeaterMode::Medium_0_1s => 0x24,
            HeaterMode::Low_1s => 0x1E,
            HeaterMode::Low_0_1s => 0x15,
        };
        let delay = match mode {
            HeaterMode::High_1s => 1000,
            HeaterMode::High_0_1s => 100,
            HeaterMode::Medium_1s => 1000,
            HeaterMode::Medium_0_1s => 100,
            HeaterMode::Low_1s => 1000,
            HeaterMode::Low_0_1s => 100,
        };
        let buf = self.inner.call::<6>(command, delay + 10).await;
        match buf {
            Ok(buf) => {
                let measurement = {
                    let temp_raw = u16::from_be_bytes([buf[0], buf[1]]); // buf[2] = CRC
                    let hum_raw = u16::from_be_bytes([buf[3], buf[4]]); // buf[5] = CRC
                    SHT40Measurement {
                        temperature: temp_raw,
                        humidity: hum_raw,
                    }
                };
                Ok(measurement)
            }
            Err(e) => Err(e),
        }
    }

    #[allow(dead_code)]
    pub async fn soft_reset(&self) -> Result<(), i2c::I2CError> {
        self.inner.command(0x94, 10).await.map(|_| ())
    }
}
