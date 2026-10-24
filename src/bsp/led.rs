use core::marker::PhantomData;

use crate::{bsp::{Initialized, Uninitialized}, gpio};

    pub struct Led<State> {
    _state: PhantomData<State>,
    port: gpio::Port,
    pin: u8,
    active_low: bool,
}

impl Led<Uninitialized> {
    pub fn init(port: gpio::Port, pin: u8, active_low: bool) -> Led<Initialized> {
        gpio::configure(port, pin, gpio::Mode::Output, gpio::Pull::None, gpio::Speed::Low, false);
        if active_low {
            gpio::set_high(port, pin);
        } else {
            gpio::set_low(port, pin);
        }
        Led { _state: PhantomData::<Initialized>, port, pin, active_low }
    }
}

impl Led<Initialized> {
    pub fn set_high(&self) {
        if self.active_low {
            gpio::set_low(self.port, self.pin);
        } else {
            gpio::set_high(self.port, self.pin);
        }
    }

    pub fn set_low(&self) {
        if self.active_low {
            gpio::set_high(self.port, self.pin);
        } else {
            gpio::set_low(self.port, self.pin);
        }
    }

    pub fn toggle(&self) {
        gpio::toggle(self.port, self.pin);
    }
}