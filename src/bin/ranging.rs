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
const CTRL_PERIOD: u32 = 6_400_000;
#[cfg(feature = "tag")]
const CTRL_PERIOD: u32 = 3_200_000;
const CTRL_PERIOD_SLOW: u32 = 10_000_000;
const BATTERY_PERIOD: u32 = 256_000_000;

#[app(device = stm32f1xx_hal::stm32, monotonic = rtic::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        dw1000: Dw1000Wrapper,
        led1: Led1Type,
        indicator: LedIndicator,
        battery_monitor: BatteryMonitor,
        ping_seen: bool,
        valid_response_seen: bool,
    }

    #[init(spawn = [control_tag, control_anchor, start_receiving, check_battery_voltage])]
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

        cx.spawn.check_battery_voltage().unwrap();

        #[cfg(feature = "anchor")]
        cx.spawn.control_anchor().unwrap();

        #[cfg(feature = "tag")]
        cx.spawn.control_tag().unwrap();

        init::LateResources {
            dw1000: Dw1000Wrapper::new(dw1000, irq),
            led1,
            indicator,
            battery_monitor,
            ping_seen: false,
            valid_response_seen: false,
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

    #[task(resources = [dw1000, led1, ping_seen, valid_response_seen], spawn = [start_receiving, set_indicator])]
    fn receive_message(cx: receive_message::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;
        let led1: &mut Led1Type = cx.resources.led1;
        let ping_seen: &mut bool = cx.resources.ping_seen;
        let valid_response_seen: &mut bool = cx.resources.valid_response_seen;

        match dw1000.receive_message() {
            Ok(Dw1000MessageType::RangingResponse(valid)) => {
                if valid {
                    led1.toggle().unwrap();
                    let average_distance = dw1000.get_average_distance();
                    defmt::info!(
                        "Received ranging response: {:?}cm ==> New filtered distance: {:?}cm",
                        dw1000.get_last_distance(),
                        average_distance
                    );
                    *valid_response_seen = true;
                    cx.spawn.set_indicator(average_distance, 100, 20).unwrap();
                }
            }
            Ok(Dw1000MessageType::Ping) => {
                defmt::info!("Received ping");
                *ping_seen = true;
            }
            Ok(message_type) => defmt::info!("Received message: {:?}", message_type),
            Err(Error::InvalidState) => {
                defmt::warn!("Dw1000 not in required state: {:?}", dw1000.get_state())
            }
            Err(e) => defmt::error!("receive_message: {:?}", e),
        };

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

    #[task(schedule = [control_anchor], spawn = [start_receiving, finish_receiving, send_ping], resources = [dw1000])]
    fn control_anchor(cx: control_anchor::Context) {
        static mut COUNT: u8 = 0;

        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;

        if let Dw1000State::Receiving = dw1000.get_state() {
            cx.spawn.finish_receiving().unwrap();
        }

        if *COUNT == 5 {
            *COUNT = 0;
            cx.spawn.send_ping().unwrap();
        } else {
            *COUNT += 1;
        }

        cx.schedule
            .control_anchor(cx.scheduled + CTRL_PERIOD.cycles())
            .unwrap();
    }

    #[task(schedule = [control_tag], spawn = [start_receiving, finish_receiving, send_ping], resources = [ping_seen, indicator, valid_response_seen])]
    fn control_tag(cx: control_tag::Context) {
        static mut COUNT: u8 = 0;
        static mut CYCLES_SINCE_PING: u8 = 255;
        static mut CYCLES_SINCE_VALID_RESPONSE: u8 = 255;
        static mut ANCHOR_DETECTED: bool = false;

        let indicator: &mut LedIndicator = cx.resources.indicator;

        let ping_seen: &mut bool = cx.resources.ping_seen;
        let valid_response_seen: &mut bool = cx.resources.valid_response_seen;

        // Anchor detection
        if *ping_seen {
            *ANCHOR_DETECTED = true;
            *CYCLES_SINCE_PING = 0;
        } else {
            *CYCLES_SINCE_PING += 1;
        }

        if *CYCLES_SINCE_PING > 50 && *ANCHOR_DETECTED {
            indicator.set_out_of_range();
            *ANCHOR_DETECTED = false;
        }

        if *COUNT == 10 || *ping_seen {
            *COUNT = 0;
        } else {
            *COUNT += 1;
        }

        *ping_seen = false;

        // Valid response detection
        if *valid_response_seen {
            *CYCLES_SINCE_VALID_RESPONSE = 0;
        } else {
            *CYCLES_SINCE_VALID_RESPONSE = (*CYCLES_SINCE_VALID_RESPONSE).saturating_add(1);
        }

        if *CYCLES_SINCE_VALID_RESPONSE > 50 {
            indicator.set_out_of_range();
        }

        *valid_response_seen = false;

        let delay_cycles = if *ANCHOR_DETECTED {
            match *COUNT {
                1 => cx.spawn.finish_receiving().unwrap(),
                9 => cx.spawn.start_receiving().unwrap(),
                _ => (),
            }
            CTRL_PERIOD.cycles()
        } else {
            match *COUNT {
                4 => cx.spawn.finish_receiving().unwrap(),
                0 => cx.spawn.start_receiving().unwrap(),
                _ => (),
            }
            CTRL_PERIOD_SLOW.cycles()
        };

        cx.schedule
            .control_tag(cx.scheduled + delay_cycles)
            .unwrap();
    }

    extern "C" {
        fn EXTI1();
        fn EXTI2();
    }
};
