pub struct SHT40Measurement {
    pub temperature: u16,
    pub humidity: u16,
}

impl SHT40Measurement {
    // Convert raw temperature and humidity values to human-readable formats
    pub fn temperature_celsius(&self) -> f32 {
        -45.0 + 175.0 * (self.temperature as f32 / 65535.0)
    }

    // Convert raw temperature and humidity values to human-readable formats
    pub fn temperature_fahrenheit(&self) -> f32 {
        -49.0 + 315.0 * (self.temperature as f32 / 65535.0)
    }

    // Convert raw temperature and humidity values to human-readable formats
    pub fn humidity_percent(&self) -> f32 {
        -6.0 + 125.0 * (self.humidity as f32 / 65535.0)
    }
}