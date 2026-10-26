//! [BMP280 datasheet](https://www.bosch-sensortec.com/media/boschsensortec/downloads/datasheets/bst-bmp280-ds001.pdf)
#[allow(dead_code)]

pub mod driver;

#[derive(Clone)]
pub struct Measurement{
    pub temperature: f32, // °C
    pub pressure: f32,    // Pa
}

#[derive(Clone, Copy, Debug, defmt::Format)]
pub struct MeasurementConfig {
    pub temperature_oversampling: Oversampling,
    pub pressure_oversampling: Oversampling,
    pub power_mode: PowerMode,
    pub iir_filter: IIRFilter,
    pub standby: Standby,
}

/// `osrs_t` / `osrs_p`: number of samples averaged per value.
#[derive(Clone, Copy, Debug, defmt::Format)]
#[repr(u8)]
pub enum Oversampling {
    Skip = 0b000, // value not measured
    X1 = 0b001,   // 16 bit / 0.0050 °C
    X2 = 0b010,   // 17 bit / 0.0025 °C
    X4 = 0b011,   // 18 bit / 0.0012 °C
    X8 = 0b100,   // 19 bit / 0.0006 °C
    X16 = 0b101,  // 20 bit / 0.0003 °C (also can be used 110, 111)
}

impl Oversampling {
    pub const fn samples(self) -> u32 {
        match self {
            Oversampling::Skip => 0,
            Oversampling::X1 => 1,
            Oversampling::X2 => 2,
            Oversampling::X4 => 4,
            Oversampling::X8 => 8,
            Oversampling::X16 => 16,
        }
    }
}

/// `mode[1:0]` in `ctrl_meas`.
#[derive(Clone, Copy, Debug, defmt::Format)]
#[repr(u8)]
pub enum PowerMode {
    Sleep = 0b00,
    Forced = 0b01,
    Normal = 0b11,
}

/// `filter[2:0]` in `config`: IIR filter coefficient, smooths short disturbances.
#[derive(Clone, Copy, Debug, defmt::Format)]
#[repr(u8)]
pub enum IIRFilter {
    Off = 0b000,
    X2 = 0b001,
    X4 = 0b010,
    X8 = 0b011,
    X16 = 0b100,
}

/// `t_sb[2:0]` in `config`: pause between measurements, normal mode only.
#[derive(Clone, Copy, Debug, defmt::Format)]
#[repr(u8)]
pub enum Standby {
    Ms0_5 = 0b000,
    Ms62_5 = 0b001,
    Ms125 = 0b010,
    Ms250 = 0b011,
    Ms500 = 0b100,
    Ms1000 = 0b101,
    Ms2000 = 0b110,
    Ms4000 = 0b111,
}

impl MeasurementConfig{
     /// Datasheet use case "weather monitoring": lowest power, one forced measurement on demand.
    pub const fn weather_monitoring() -> Self {
        Self {
            temperature_oversampling: Oversampling::X1,
            pressure_oversampling: Oversampling::X1,
            power_mode: PowerMode::Forced,
            iir_filter: IIRFilter::Off,
            standby: Standby::Ms0_5, // unused in forced mode
        }
    }

    /// Datasheet use case "indoor navigation": highest resolution, continuous.
    pub const fn indoor_navigation() -> Self {
        Self {
            temperature_oversampling: Oversampling::X2,
            pressure_oversampling: Oversampling::X16,
            power_mode: PowerMode::Normal,
            iir_filter: IIRFilter::X16,
            standby: Standby::Ms0_5,
        }
    }

    /// Value for register `ctrl_meas` (0xF4): osrs_t[7:5] | osrs_p[4:2] | mode[1:0].
    pub const fn ctrl_meas(self) -> u8 {
        ((self.temperature_oversampling as u8) << 5) | ((self.pressure_oversampling as u8) << 2) | self.power_mode as u8
    }

    /// Value for register `config` (0xF5): t_sb[7:5] | filter[4:2] | spi3w_en[0] = 0.
    /// Write it while the chip is in sleep mode; writes in normal mode may be ignored.
    pub const fn config(self) -> u8 {
        ((self.standby as u8) << 5) | ((self.iir_filter as u8) << 2)
    }

    /// Maximum measurement time in µs (datasheet section 3.8.1, "Measurement time").
    pub const fn max_measurement_time_us(self) -> u32 {
        let t = self.temperature_oversampling.samples();
        let p = self.pressure_oversampling.samples();
        let pressure_us = if p > 0 { 2_300 * p + 575 } else { 0 };
        1_250 + 2_300 * t + pressure_us
    }
}

impl Default for MeasurementConfig {
    fn default() -> Self {
        Self::weather_monitoring()
    }
}