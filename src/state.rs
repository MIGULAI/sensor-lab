use crate::bsp::{SensorState, bmp280::{Measurement, MeasurementConfig}, sht40::measurement::SHT40Measurement};
    

#[derive(Clone)]
pub struct State {
    pub bmp280_state: Option<SensorState<Measurement, MeasurementConfig>>,
    pub sht40_state: Option<SensorState<SHT40Measurement, Option<()>>>,
}