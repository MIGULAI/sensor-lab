use crate::Mono;
use crate::bsp::ssd1306_display::drawings::FrameBuffer;
use crate::bsp::ssd1306_display::driver::SSD1306Display;
use crate::{app::display_task, screens::all_in_one};
use rtic::Mutex; // needed for `.lock()` outside the app module
use rtic_monotonics::systick::prelude::*;

pub async fn display_task(mut cx: display_task::Context<'_>) {
    let bus = *cx.local.display_bus;
    loop {
        let display = loop {
            match SSD1306Display::new(bus).init().await {
                Ok(display) => break display,
                Err(e) => {
                    defmt::error!("Display: failed to initialize: {:?}", e);
                    Mono::delay(1000.millis()).await;
                }
            }
        };
        Mono::delay(1000.millis()).await; // wait for logo to show
        let mut fb = FrameBuffer::new();
        loop {
            let state = cx.shared.state.lock(|state| state.clone());
            all_in_one::render(&mut fb, &state); // no lock held while drawing
            if let Err(e) = display.draw(&fb.0).await {
                defmt::error!("Display: draw failed: {:?}", e);
                break;
            }
            Mono::delay(1000.millis()).await;
        }
    }
}
