//! [SHT40 datasheet](https://download.mikroe.com/documents/datasheets/SHT40%20Datasheet.pdf)

pub mod driver;
pub mod measurement;

#[allow(dead_code)]
pub enum Precision {
    High,
    Medium,
    Low,
}

#[allow(non_camel_case_types, dead_code)]
pub enum HeaterMode {
    // activate highest heater power & high precis. meas. (typ. 200mW @ 3.3V) for 1s
    High_1s,
    // activate highest heater power & high precis. meas. (typ. 200mW @ 3.3V) for 0.1s
    High_0_1s,
    // activate medium heater power & medium precis. meas. (typ. 110mW @ 3.3V) for 1s
    Medium_1s,
    // activate medium heater power & medium precis. meas. (typ. 110mW @ 3.3V) for 0.1s
    Medium_0_1s,
    // activate lowest heater power & low precis. meas. (typ. 20mW @ 3.3V) for 1s
    Low_1s,
    // activate lowest heater power & low precis. meas. (typ. 20mW @ 3.3V) for 0.1s
    Low_0_1s,
}