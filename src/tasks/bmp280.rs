use crate::{
    Mono,
    app::bmp280_task,
    bsp::{
        SensorState,
        bmp280::{MeasurementConfig, driver::BMP280},
    },
};
use rtic::Mutex; // needed for `.lock()` outside the app module

use rtic_monotonics::systick::prelude::*;

pub async fn bmp280_task(mut cx: bmp280_task::Context<'_>) {
    let bus = *cx.local.bmp280_bus;
    let bmp280 = match BMP280::new(bus).init().await {
        Ok(bmp280) => bmp280,
        Err(e) => {
            defmt::error!("BMP280: failed to initialize: {:?}", e);
            cx.shared.state.lock(|state| {
                state.bmp280_state = Some(SensorState {
                    measurements: None,
                    last_measurement_time: None,
                    id: None,
                    healthy: false,
                    error: true,
                    config: None,
                });
            });
            return;
        }
    };
    let config = MeasurementConfig::weather_monitoring();

    cx.shared.state.lock(|state| {
        state.bmp280_state = Some(SensorState {
            measurements: None,
            last_measurement_time: None,
            id: Some(bmp280.chip_id as u32),
            healthy: true,
            error: false,
            config: Some(config),
        });
    });
    bmp280.set_configuration(config).await.ok();
    loop {
        bmp280.start_measurement(config).await.ok();
        let measurement = bmp280.get_temperature_pressure().await;
        if let Ok(measurement) = measurement {
            cx.shared.state.lock(|state| {
                state.bmp280_state.as_mut().map(|s| {
                    s.healthy = true;
                    s.error = false;
                    s.measurements = Some(measurement);
                    s.last_measurement_time = Some(Mono::now().ticks());
                });
            });
        } else {
            defmt::error!("BMP280: failed to get measurement: {:?}", measurement.err());
            cx.shared.state.lock(|state| {
                state.bmp280_state.as_mut().map(|s| {
                    s.healthy = false;
                    s.error = true;
                });
            });
        }
        Mono::delay(1000.millis()).await;
    }
}
