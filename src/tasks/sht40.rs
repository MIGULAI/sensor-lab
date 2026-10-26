use crate::{Mono, app::sht40_task, bsp::{SensorState, sht40::{Precision, driver::SHT40}}};
use rtic::Mutex; // needed for `.lock()` outside the app module
use rtic_monotonics::systick::prelude::*;

pub async fn sht40_task(mut cx: sht40_task::Context<'_>) {
    let sht40 = match SHT40::new(*cx.local.sht40_bus).init().await {
        Ok(sht40) => sht40,
        Err(e) => {
            defmt::error!("SHT40: failed to initialize: {:?}", e);
            cx.shared.state.lock(|state| {
                state.sht40_state = Some(SensorState {
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
    cx.shared.state.lock(|state| {
        state.sht40_state = Some(SensorState {
            measurements: None,
            last_measurement_time: None,
            id: Some(sht40.serial),
            healthy: true,
            error: false,
            config: None,
        });
    });
    loop {
        let measurement = sht40.get_temperature_humidity(Precision::High).await;
        if let Ok(measurement) = measurement {
            cx.shared.state.lock(|state| {
                state.sht40_state.as_mut().map(|s| {
                    s.healthy = true;
                    s.error = false;
                    s.measurements = Some(measurement);
                    s.last_measurement_time = Some(Mono::now().ticks());
                });
            });
        } else {
            defmt::error!("SHT40: failed to get measurement: {:?}", measurement.err());
            cx.shared.state.lock(|state| {
                state.sht40_state.as_mut().map(|s| {
                    s.healthy = false;
                    s.error = true;
                });
            });
        }
        Mono::delay(1000.millis()).await;
    }
}
