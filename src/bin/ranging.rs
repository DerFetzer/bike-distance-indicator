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
#[cfg(feature = "anchor")]
use rtic::cyccnt::U32Ext;

#[cfg(feature = "anchor")]
const ADDRESS: u16 = 0x1234;
#[cfg(feature = "tag")]
const ADDRESS: u16 = 0x1235;

#[cfg(feature = "anchor")]
const CTRL_PERIOD: u32 = 64_000_000;

#[app(device = stm32f1xx_hal::stm32, monotonic = rtic::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        dw1000: Dw1000Wrapper,
        led1: Led1Type,
    }

    #[init(spawn = [control, start_receiving])]
    fn init(mut cx: init::Context) -> init::LateResources {
        cx.core.DWT.enable_cycle_counter();

        defmt::info!("Hello, RTIC!");

        let dp = cx.device;
        let cp = cx.core;

        let (mut dw1000, irq, led1) = init_hardware(dp, cp);

        defmt::info!("Set address");

        // Set network address
        dw1000
            .set_address(
                mac::PanId(0x0d57),         // hardcoded network id
                mac::ShortAddress(ADDRESS), // random device address
            )
            .expect("Failed to set address");

        cx.spawn.start_receiving().unwrap();

        #[cfg(feature = "anchor")]
        cx.spawn.control().unwrap();

        init::LateResources {
            dw1000: Dw1000Wrapper::new(dw1000, irq),
            led1,
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

    #[task(resources = [dw1000, led1], spawn = [start_receiving])]
    fn receive_message(cx: receive_message::Context) {
        let dw1000: &mut Dw1000Wrapper = cx.resources.dw1000;
        let led1: &mut Led1Type = cx.resources.led1;

        defmt::info!("state before receive: {:?}", dw1000.get_state());

        match dw1000.receive_message() {
            Ok(Dw1000MessageType::RangingResponse) => {
                led1.toggle().unwrap();
                defmt::info!(
                    "Received ranging response: {:?}cm ==> New filtered distance: {:?}cm",
                    dw1000.get_last_distance(),
                    dw1000.get_average_distance()
                );
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
