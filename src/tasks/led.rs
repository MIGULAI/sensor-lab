use rtic_monotonics::systick::prelude::*;

use crate::{Mono, app::blink};

pub async fn blink(cx: blink::Context<'_>) {
    loop {
        cx.local.led.toggle();
        Mono::delay(500.millis()).await;
    }
}
