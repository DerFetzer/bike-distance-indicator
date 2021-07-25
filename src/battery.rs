use crate::types::{BatteryAdcType, BatteryChType};
use embedded_hal::adc::OneShot;

const SUPPLY_VOLTAGE: u32 = 3300;
const LOW_BAT_THRESHOLD: u32 = 3450;
const VOLTAGE_FACTOR: u32 = 2;

pub enum BatteryState {
    Ok(u16),
    Empty(u16),
    Unknown,
}

pub struct BatteryMonitor {
    adc: BatteryAdcType,
    ch: BatteryChType,
}

impl BatteryMonitor {
    pub fn new(adc: BatteryAdcType, ch: BatteryChType) -> Self {
        BatteryMonitor {
            adc,
            ch,
        }
    }

    pub fn read_battery_voltage(&mut self) -> u16 {
        let reading: u16 = self.adc.read(&mut self.ch).unwrap();
        self.reading_to_battery_voltage(reading)
    }

    pub fn check_battery(&mut self) -> BatteryState {
        match self.read_battery_voltage() {
            v if v > LOW_BAT_THRESHOLD as u16 => BatteryState::Ok(v),
            v => BatteryState::Empty(v),
        }
    }

    fn reading_to_battery_voltage(&self, reading: u16) -> u16 {
        let reading_voltage = reading as u32 * SUPPLY_VOLTAGE / self.adc.max_sample() as u32;
        (reading_voltage * VOLTAGE_FACTOR) as u16
    }
}