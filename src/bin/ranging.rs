#![no_main]
#![no_std]

#[cfg(all(feature = "anchor", feature = "tag"))]
compile_error!("feature \"anchor\" and feature \"tag\" cannot be enabled at the same time");

use bike_distance_indicator as _;
use bike_distance_indicator::init::init_hardware;

use rtic::app;

use bike_distance_indicator::dw1000::{Dw1000MessageType, Dw1000State, Dw1000Wrapper};
use bike_distance_indicator::error::Error;
use bike_distance_indicator::types::Led1Type;
use dw1000::mac;
use embedded_hal::digital::v2::ToggleableOutputPin;
use rtic::cyccnt::U32Ext;
use bike_distance_indicator::indicator::{LedIndicator, DistanceIndicator};
use bike_distance_indicator::battery::{BatteryMonitor, BatteryState};
use stm32f1xx_hal::pac::PWR;
use bike_distance_indicator::helper::get_delay;
use embedded_hal::blocking::delay::DelayMs;

#[cfg(feature = "anchor")]
const ADDRESS: u16 = 0x1234;
#[cfg(feature = "tag")]
const ADDRESS: u16 = 0x1235;

#[cfg(feature = "anchor")]
const CTRL_PERIOD: u32 = 64_000_000;
const BATTERY_PERIOD: u32 = 256_000_000;

#[app(device = stm32f1xx_hal::stm32, monotonic = rtic::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        dw1000: Dw1000Wrapper,
        led1: Led1Type,
        indicator: LedIndicator,
        battery_monitor: BatteryMonitor,
    }

    #[init(spawn = [control, start_receiving, check_battery_voltage])]
    fn init(mut cx: init::Context) -> init::LateResources {
        cx.core.DWT.enable_cycle_counter();

        defmt::info!("Hello, RTIC!");

        let dp = cx.device;
        let cp = cx.core;

        let (mut dw1000, irq, led1, indicator, battery_monitor) = init_hardware(dp, cp);

        defmt::info!("Set address");

        // Set network address
        dw1000
            .set_address(
                mac::PanId(0x0d57),         // hardcoded network id
                mac::ShortAddress(ADDRESS), // random device address
            )
            .expect("Failed to set address");

        cx.spawn.start_receiving().unwrap();
        cx.spawn.check_battery_voltage().unwrap();

        #[cfg(feature = "anchor")]
        cx.spawn.control().unwrap();

        init::LateResources {
            dw1000: Dw1000Wrapper::new(dw1000, irq),
            led1,
            indicator,
            battery_monitor,
        }
    }

    #[task(resources = [dw1000])]
    fn send_ping(cx: send_ping::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        match dw1000.send_ping() {
            Ok(()) => {}
            Err(Error::InvalidState) => {
                defmt::warn!("Dw1000 not in required state: {:?}", dw1000.get_state())
            }
            Err(e) => defmt::error!("send_ping: {:?}", e),
        }
    }

    #[task(resources = [dw1000])]
    fn start_receiving(cx: start_receiving::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        match dw1000.start_receiving() {
            Ok(()) => {}
            Err(Error::InvalidState) => {
                defmt::warn!("Dw1000 not in required state: {:?}", dw1000.get_state())
            }
            Err(e) => defmt::error!("start_receiving: {:?}", e),
        }
    }

    #[task(resources = [dw1000])]
    fn finish_receiving(cx: finish_receiving::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        match dw1000.finish_receiving() {
            Ok(()) => {}
            Err(Error::InvalidState) => {
                defmt::warn!("Dw1000 not in required state: {:?}", dw1000.get_state())
            }
            Err(e) => defmt::error!("finish_receiving: {:?}", e),
        }
    }

    #[task(resources = [dw1000])]
    fn finish_sending(cx: finish_sending::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        match dw1000.finish_sending() {
            Ok(()) => {}
            Err(Error::InvalidState) => {
                defmt::warn!("Dw1000 not in required state: {:?}", dw1000.get_state())
            }
            Err(e) => defmt::error!("finish_sending: {:?}", e),
        }
    }

    #[task(resources = [dw1000, led1], spawn = [start_receiving, set_indicator])]
    fn receive_message(cx: receive_message::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;
        let led1: &mut Led1Type = cx.resources.led1;

        defmt::info!("state before receive: {:?}", dw1000.get_state());

        match dw1000.receive_message() {
            Ok(Dw1000MessageType::RangingResponse) => {
                led1.toggle().unwrap();
                let average_distance = dw1000.get_average_distance();
                defmt::info!(
                    "Received ranging response: {:?}cm ==> New filtered distance: {:?}cm",
                    dw1000.get_last_distance(),
                    average_distance
                );
                cx.spawn.set_indicator(average_distance, 100, 20).unwrap();
            }
            Ok(message_type) => defmt::info!("Received message: {:?}", message_type),
            Err(Error::InvalidState) => {
                defmt::warn!("Dw1000 not in required state: {:?}", dw1000.get_state())
            }
            Err(e) => defmt::error!("receive_message: {:?}", e),
        };
        defmt::info!("after receive_message state: {:?}", dw1000.get_state());
        if dw1000.get_state() != Dw1000State::Sending {
            cx.spawn.start_receiving().unwrap();
        }
    }

    #[task(resources = [indicator])]
    fn set_indicator(cx: set_indicator::Context, current_distance: u64, target_distance: u64, tolerance: u64) {
        let indicator: &mut LedIndicator = cx.resources.indicator;

        indicator.update_range(current_distance, target_distance, tolerance).unwrap();
    }

    #[task(binds = EXTI0, resources = [dw1000], spawn = [receive_message, start_receiving, finish_sending])]
    fn exti2(cx: exti2::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        dw1000.handle_interrupt().unwrap();

        match dw1000.get_state() {
            Dw1000State::Ready => defmt::warn!("Interrupt in ready state"),
            Dw1000State::Sending => {
                cx.spawn.finish_sending().unwrap();
                cx.spawn.start_receiving().unwrap();
            }
            Dw1000State::Receiving => {
                cx.spawn.receive_message().unwrap();
            }
        }
    }

    #[task(resources = [battery_monitor], spawn = [shutdown], schedule = [check_battery_voltage])]
    fn check_battery_voltage(cx: check_battery_voltage::Context) {
        let battery_monitor: &mut BatteryMonitor = cx.resources.battery_monitor;

        match battery_monitor.check_battery() {
            BatteryState::Ok(v) => {
                defmt::info!("Battery Ok, voltage: {:?}mV", v);
            }
            BatteryState::Empty(v) => {
                defmt::info!("Battery Empty, voltage: {:?}mV", v);
                cx.spawn.shutdown().unwrap();
            }
            BatteryState::Unknown => {}
        };

        cx.schedule
            .check_battery_voltage(cx.scheduled + BATTERY_PERIOD.cycles())
            .unwrap();
    }

    #[task(resources = [indicator, dw1000])]
    fn shutdown(cx: shutdown::Context) {
        let indicator: &mut LedIndicator = cx.resources.indicator;
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        let mut delay = get_delay();

        defmt::error!("Shutdown!");

        delay.delay_ms(500u32);

        dw1000.shutdown();
        indicator.shutdown();

        unsafe {
            let mut cmp = cortex_m::Peripherals::steal();
            cmp.SCB.set_sleepdeep();
            (*PWR::ptr()).cr.write(|w| w.pdds().set_bit().cwuf().set_bit());

            cortex_m::asm::wfi();
            unreachable!();
        }
    }

    #[cfg(feature = "anchor")]
    #[task(schedule = [control], spawn = [start_receiving, finish_receiving, send_ping], resources = [dw1000])]
    fn control(cx: control::Context) {
        #[cfg(feature = "anchor")]
        static mut COUNT: u8 = 0;
        #[cfg(feature = "anchor")]
        static mut RX_ACTIVE: bool = false;

        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        if let Dw1000State::Receiving = dw1000.get_state() {
            if !*RX_ACTIVE {
                *COUNT = 0
            }

            *RX_ACTIVE = true;

            if *COUNT == 2 {
                cx.spawn.finish_receiving().unwrap();
                cx.spawn.send_ping().unwrap();
            } else {
                *COUNT += 1;
            }
        } else {
            *RX_ACTIVE = false;
        }

        cx.schedule
            .control(cx.scheduled + CTRL_PERIOD.cycles())
            .unwrap();
    }

    extern "C" {
        fn EXTI1();
        fn EXTI2();
    }
};
