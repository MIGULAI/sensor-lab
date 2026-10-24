use core::marker::PhantomData;

use rtic_monotonics::systick::prelude::*;
use rtic_sync::arbiter::Arbiter;
use stm32f4::stm32f411::I2C1;

use crate::{
    Mono,
    bsp::{Initialized, Uninitialized, bmp280::MeasurementConfig},
    i2c,
};

pub struct Trimming {
    pub dig_t1: u16,
    pub dig_t2: i16,
    pub dig_t3: i16,
    pub dig_p1: u16,
    pub dig_p2: i16,
    pub dig_p3: i16,
    pub dig_p4: i16,
    pub dig_p5: i16,
    pub dig_p6: i16,
    pub dig_p7: i16,
    pub dig_p8: i16,
    pub dig_p9: i16,
}

pub struct BMP280<'a, State> {
    _state: PhantomData<State>,
    pub bus: &'a Arbiter<I2C1>,
    pub chip_id: u8,
    pub trimming: Trimming,
}

pub struct BMP280Measurement {
    pub temperature: Option<f32>,
    pub pressure: f32,
}

impl<'a, State> BMP280<'a, State> {
    // `delay_ms`: time to wait between command and read-back (measurement/heater duration).
    // The bus is only held for the write and the read, so other devices can use it while the sensor measures.
    // `N`: number of bytes to read back, chosen by the caller at compile time.
    async fn call<const N: usize>(
        &self,
        command: u8,
        delay_ms: u32,
    ) -> Result<[u8; N], i2c::I2CError> {
        let result = i2c::write_byte(&*self.bus.access().await, 0x76, command);
        if let Err(e) = result {
            defmt::error!("BMP280: write error: {:?}", e);
            return Err(e);
        } else {
            Mono::delay(delay_ms.millis()).await;
            let mut buf = [0u8; N];
            match i2c::read(&*self.bus.access().await, 0x76, &mut buf) {
                Ok(()) => {
                    return Ok(buf);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }
}

impl<'a> BMP280<'a, Uninitialized> {
    async fn get_chip_id(&self) -> Result<u8, i2c::I2CError> {
        self.call::<1>(0xD0, 0).await.map(|buf| buf[0])
    }

    async fn get_trimming(&self) -> Result<Trimming, i2c::I2CError> {
        self.call::<24>(0x88, 0).await.map(|buf| {
            let dig_t1 = u16::from_le_bytes([buf[0], buf[1]]);
            let dig_t2 = i16::from_le_bytes([buf[2], buf[3]]);
            let dig_t3 = i16::from_le_bytes([buf[4], buf[5]]);
            let dig_p1 = u16::from_le_bytes([buf[6], buf[7]]);
            let dig_p2 = i16::from_le_bytes([buf[8], buf[9]]);
            let dig_p3 = i16::from_le_bytes([buf[10], buf[11]]);
            let dig_p4 = i16::from_le_bytes([buf[12], buf[13]]);
            let dig_p5 = i16::from_le_bytes([buf[14], buf[15]]);
            let dig_p6 = i16::from_le_bytes([buf[16], buf[17]]);
            let dig_p7 = i16::from_le_bytes([buf[18], buf[19]]);
            let dig_p8 = i16::from_le_bytes([buf[20], buf[21]]);
            let dig_p9 = i16::from_le_bytes([buf[22], buf[23]]);

            Trimming {
                dig_t1,
                dig_t2,
                dig_t3,
                dig_p1,
                dig_p2,
                dig_p3,
                dig_p4,
                dig_p5,
                dig_p6,
                dig_p7,
                dig_p8,
                dig_p9,
            }
        })
    }

    pub fn new(bus: &'a Arbiter<I2C1>) -> Self {
        BMP280 {
            _state: PhantomData::<Uninitialized>,
            bus,
            chip_id: 0,
            trimming: Trimming {
                dig_t1: 0,
                dig_t2: 0,
                dig_t3: 0,
                dig_p1: 0,
                dig_p2: 0,
                dig_p3: 0,
                dig_p4: 0,
                dig_p5: 0,
                dig_p6: 0,
                dig_p7: 0,
                dig_p8: 0,
                dig_p9: 0,
            },
        }
    }

    // Consumes the uninitialized driver and returns the initialized one.
    pub async fn init(self) -> Result<BMP280<'a, Initialized>, i2c::I2CError> {
        let chip_id = match self.get_chip_id().await {
            Ok(chip_id) => chip_id,
            Err(e) => {
                defmt::error!("BMP280: failed to get chip ID: {:?}", e);
                return Err(e);
            }
        };
        if chip_id != 0x58 {
            defmt::error!("BMP280: unexpected chip ID: {=u8:#x}", chip_id);
            return Err(i2c::I2CError::UnexpectedDevice);
        }
        let trimming = self.get_trimming().await;
        let trimming = match trimming {
            Ok(trimming) => trimming,
            Err(e) => {
                defmt::error!("BMP280: failed to get trimming parameters: {:?}", e);
                return Err(e);
            }
        };

        Ok(BMP280 {
            _state: PhantomData::<Initialized>,
            bus: self.bus,
            chip_id,
            trimming: trimming,
        })
    }
}

impl<'a> BMP280<'a, Initialized> {
    fn compensate_temperature(&self, adc_t: u32) -> (f32, u32) {
        let var1 = (((adc_t >> 3) - ((self.trimming.dig_t1 as u32) << 1))
            * (self.trimming.dig_t2 as u32))
            >> 11;
        let var2 = (((((adc_t >> 4) - (self.trimming.dig_t1 as u32))
            * ((adc_t >> 4) - (self.trimming.dig_t1 as u32)))
            >> 12)
            * (self.trimming.dig_t3 as u32))
            >> 14;
        let t_fine = var1 + var2;
        let temperature = (t_fine * 5 + 128) >> 8;
        (temperature as f32 / 100.0, t_fine)
    }

    fn compensate_pressure(&self, adc_p: u32, t_fine: u32) -> f32 {
        let var1 = (t_fine as i64) - 128000;
        let var2 = var1 * var1 * (self.trimming.dig_p6 as i64);
        let var2 = var2 + ((var1 * (self.trimming.dig_p5 as i64)) << 17);
        let var2 = var2 + ((self.trimming.dig_p4 as i64) << 35);
        let var1 = ((var1 * var1 * (self.trimming.dig_p3 as i64)) >> 8)
            + ((var1 * (self.trimming.dig_p2 as i64)) << 12);
        let var1 = ((((1i64) << 47) + var1) * (self.trimming.dig_p1 as i64)) >> 33;

        if var1 == 0 {
            return 0.0; // avoid division by zero
        }
        let p = 1048576 - adc_p as i64;
        let p = (((p << 31) - var2) * 3125) / var1;
        let var1 = ((self.trimming.dig_p9 as i64) * (p >> 13) * (p >> 13)) >> 25;
        let var2 = ((self.trimming.dig_p8 as i64) * p) >> 19;
        let pressure = ((p + var1 + var2) >> 8) + ((self.trimming.dig_p7 as i64) << 4);

        pressure as f32 / 256.0
    }
    pub async fn set_configuration(&self, config: MeasurementConfig) -> Result<(), i2c::I2CError> {
        i2c::write(&*self.bus.access().await, 0x76, &[0xF5, config.config()])?;
        Ok(())
    }

    pub async fn start_measurement(&self, config: MeasurementConfig) -> Result<(), i2c::I2CError> {
        i2c::write(&*self.bus.access().await, 0x76, &[0xF4, config.ctrl_meas()])?;
        // Forced-mode conversion at x1/x1 takes up to 6.4 ms (datasheet table 13).
        Mono::delay((config.max_measurement_time_us() / 1000 + 1).millis()).await;
        Ok(())
    }

    pub async fn get_temperature_pressure(&self) -> Result<(f32, f32), i2c::I2CError> {
        let buf = self.call::<6>(0xF7, 10).await?;
        let adc_p = ((buf[0] as u32) << 12) | ((buf[1] as u32) << 4) | ((buf[2] as u32) >> 4);
        let adc_t = ((buf[3] as u32) << 12) | ((buf[4] as u32) << 4) | ((buf[5] as u32) >> 4);

        let (temperature, t_fine) = self.compensate_temperature(adc_t);
        let pressure = self.compensate_pressure(adc_p, t_fine);

        Ok((temperature, pressure))
    }
}
