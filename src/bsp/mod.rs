pub mod led;
pub mod sht40;
pub mod bmp280;
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