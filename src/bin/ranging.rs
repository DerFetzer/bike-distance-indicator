//! Range measurement tag node
//!
//! This is a tag node used for range measurement. Tags use anchor nodes to
//! measure their distance from those anchors.
//!
//! Currently, distance measurements have a highly inaccurate result. One reason
//! that could account for this is the lack of antenna delay calibration, but
//! it's possible that there are various hidden bugs that contribute to this.

#![no_main]
#![no_std]

#[cfg(all(feature = "anchor", feature = "tag"))]
compile_error!("feature \"anchor\" and feature \"tag\" cannot be enabled at the same time");

use bike_distance_indicator as _;
use bike_distance_indicator::init::init_hardware;

use rtic::app;

use dw1000::{
    mac,
    ranging::{self, Message as _RangingMessage},
    RxConfig,
};

use bike_distance_indicator::helper::get_delay;
use bike_distance_indicator::types::{
    DwCsType, DwIrqType, DwSpiType, DwTypeReady, DwTypeReceiving, DwTypeSending,
};
use embedded_hal::blocking::delay::{DelayMs, DelayUs};
#[cfg(feature = "anchor")]
use rtic::cyccnt::U32Ext;
use stm32f1xx_hal::gpio::ExtiPin;

#[cfg(feature = "anchor")]
const ADDRESS: u16 = 0x1234;
#[cfg(feature = "tag")]
const ADDRESS: u16 = 0x1235;

#[cfg(feature = "anchor")]
const CTRL_PERIOD: u32 = 64_000_000;

#[app(device = stm32f1xx_hal::stm32, monotonic = rtic::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        dw1000_ready: Option<DwTypeReady>,
        dw1000_sending: Option<DwTypeSending>,
        dw1000_receiving: Option<DwTypeReceiving>,
        irq: DwIrqType,
    }

    #[init(spawn = [control, start_receiving])]
    fn init(mut cx: init::Context) -> init::LateResources {
        cx.core.DWT.enable_cycle_counter();

        defmt::info!("Hello, RTIC!");

        let dp = cx.device;
        let cp = cx.core;

        let (mut dw1000, irq) = init_hardware(dp, cp);

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
            dw1000_ready: Some(dw1000),
            dw1000_sending: None,
            dw1000_receiving: None,
            irq,
        }
    }

    #[task(resources = [dw1000_ready, dw1000_sending])]
    fn send_ping(cx: send_ping::Context) {
        let dw1000_ready: &mut Option<DwTypeReady> = cx.resources.dw1000_ready;

        if let Some(mut dw1000) = dw1000_ready.take() {
            defmt::info!("Sending ping...");

            let sending = ranging::Ping::new(&mut dw1000)
                .expect("Failed to initiate ping")
                .send(dw1000)
                .expect("Failed to initiate ping transmission");

            *cx.resources.dw1000_sending = Some(sending);
        }
    }

    #[task(resources = [dw1000_ready, dw1000_sending, dw1000_receiving])]
    fn start_receiving(cx: start_receiving::Context) {
        let dw1000_ready: &mut Option<DwTypeReady> = cx.resources.dw1000_ready;

        if let Some(dw1000) = dw1000_ready.take() {
            defmt::info!("Start receiving");

            let receiving = dw1000
                .receive(RxConfig::default())
                .expect("Failed to receive message");

            *cx.resources.dw1000_receiving = Some(receiving);
        }
    }

    #[task(resources = [dw1000_ready, dw1000_receiving])]
    fn finish_receiving(cx: finish_receiving::Context) {
        let dw1000_receiving: &mut Option<DwTypeReceiving> = cx.resources.dw1000_receiving;

        if let Some(dw1000) = dw1000_receiving.take() {
            defmt::info!("Finish receiving");

            let ready = dw1000.finish_receiving().unwrap();

            *cx.resources.dw1000_ready = Some(ready);
        }
    }

    #[task(resources = [dw1000_ready, dw1000_receiving, dw1000_sending], spawn = [start_receiving])]
    fn receive_message(cx: receive_message::Context) {
        let dw1000_receiving: &mut Option<DwTypeReceiving> = cx.resources.dw1000_receiving;

        let mut delay = get_delay();

        if let Some(mut dw1000) = dw1000_receiving.take() {
            let mut buf = [0; 128];

            defmt::info!("Receive message");

            delay.delay_us(1000u32);

            let result = dw1000.wait(&mut buf);

            let ready = dw1000.finish_receiving().unwrap();

            *cx.resources.dw1000_ready = Some(ready);

            match result {
                Ok(message) => {
                    let dw1000_ready: &mut Option<DwTypeReady> = cx.resources.dw1000_ready;

                    if let Some(mut dw1000) = dw1000_ready.take() {
                        let ping = ranging::Ping::decode::<DwSpiType, DwCsType>(&message);
                        let request = ranging::Request::decode::<DwSpiType, DwCsType>(&message);
                        let response = ranging::Response::decode::<DwSpiType, DwCsType>(&message);

                        if let Ok(Some(ping)) = ping {
                            defmt::info!("Sending ranging request...");

                            delay.delay_ms(10u32);

                            let sending = ranging::Request::new(&mut dw1000, &ping)
                                .expect("Failed to initiate request")
                                .send(dw1000)
                                .expect("Failed to initiate request transmission");

                            *cx.resources.dw1000_sending = Some(sending);
                        } else if let Ok(Some(request)) = request {
                            defmt::info!("Sending ranging response...");

                            delay.delay_ms(10u32);

                            let sending = ranging::Response::new(&mut dw1000, &request)
                                .expect("Failed to initiate response")
                                .send(dw1000)
                                .expect("Failed to initiate response transmission");

                            *cx.resources.dw1000_sending = Some(sending);
                        } else if let Ok(Some(response)) = response {
                            defmt::info!("Received ranging response");

                            let ping_rt = response.payload.ping_reply_time.value();
                            let ping_rtt = response.payload.ping_round_trip_time.value();
                            let request_rt = response.payload.request_reply_time.value();
                            let request_rtt = response
                                .rx_time
                                .duration_since(response.payload.request_tx_time)
                                .value();

                            defmt::debug!(
                                "ping_rt: {:?} ping_rtt: {:?} request_rt: {:?} request_rtt: {:?}",
                                ping_rt,
                                ping_rtt,
                                request_rt,
                                request_rtt
                            );

                            // If this is not a PAN ID and short address, it doesn't
                            // come from a compatible node. Ignore it.
                            if let mac::Address::Short(pan_id, addr) = response.source {
                                // Ranging response received. Compute distance.
                                let distance_mm = ranging::compute_distance_mm(&response);

                                if let Ok(distance_mm) = distance_mm {
                                    let distance_cm = distance_mm / 10;
                                    // Simple correction based on https://github.com/braun-embedded/rust-dw1000/issues/105
                                    //
                                    // <corrected distance> = <measured distance> + <range bias>
                                    // <range bias> = <base part> + <distance-dependent part>
                                    //
                                    // <basepart> = -23 cm // for 16 MHz PRF, narrow-band channel
                                    //
                                    // Linear Regression:
                                    //
                                    // <measured distance> <= 1200: (30/1200)*x
                                    // <measured distance> >  1200: (6/2500) *x + 27.12
                                    let dep_part = if distance_cm <= 1200 {
                                        (30f64 / 1200f64) * distance_cm as f64
                                    }
                                    else {
                                        (6f64 / 2500f64) * distance_cm as f64 + 27.12f64
                                    };
                                    let corrected_distance = distance_cm as f64 - 23f64 + dep_part;
                                    defmt::info!("{:04x}:{:04x} - {} cm - uncorrected {} cm", pan_id.0, addr.0, corrected_distance as u32, distance_cm);
                                } else {
                                    defmt::warn!(
                                        "Could not compute distance from {:04x}:{:04x}",
                                        pan_id.0,
                                        addr.0
                                    );
                                }
                            }

                            cx.spawn.start_receiving().unwrap();
                            *cx.resources.dw1000_ready = Some(dw1000)
                        } else {
                            defmt::info!("Ignoring unknown message");
                            cx.spawn.start_receiving().unwrap();
                            *cx.resources.dw1000_ready = Some(dw1000)
                        };
                    }
                }
                Err(_) => {
                    defmt::info!("Could not receive message");
                    cx.spawn.start_receiving().unwrap();
                }
            };
        }
    }

    #[task(binds = EXTI0, resources = [dw1000_ready, dw1000_receiving, dw1000_sending, irq], spawn = [receive_message, start_receiving])]
    fn exti2(cx: exti2::Context) {
        let irq: &mut DwIrqType = cx.resources.irq;

        let dw1000_sending: &mut Option<DwTypeSending> = cx.resources.dw1000_sending;
        let dw1000_receiving: &mut Option<DwTypeReceiving> = cx.resources.dw1000_receiving;

        irq.clear_interrupt_pending_bit();

        if let Some(dw1000) = dw1000_sending.take() {
            defmt::info!("Finish sending");

            let ready = dw1000.finish_sending().expect("Failed to finish sending");
            cx.spawn.start_receiving().unwrap();

            *cx.resources.dw1000_ready = Some(ready);
        } else if let Some(_) = dw1000_receiving {
            cx.spawn.receive_message().unwrap();
        }
    }

    #[cfg(feature = "anchor")]
    #[task(schedule = [control], spawn = [start_receiving, finish_receiving, send_ping], resources = [dw1000_receiving])]
    fn control(cx: control::Context) {
        #[cfg(feature = "anchor")]
        static mut COUNT: u8 = 0;
        #[cfg(feature = "anchor")]
        static mut RX_ACTIVE: bool = false;

        let dw1000_receiving: &mut Option<DwTypeReceiving> = cx.resources.dw1000_receiving;

        if dw1000_receiving.is_some() {
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
