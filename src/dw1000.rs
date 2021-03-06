use crate::error::Error;
use crate::helper::get_delay;
use crate::types::{DwCsType, DwIrqType, DwSpiType, DwTypeReady, DwTypeReceiving, DwTypeSending};
use defmt::Format;
use dw1000::ranging::Message;
use dw1000::{mac, ranging, RxConfig};
use embedded_hal::blocking::delay::DelayUs;
use stm32f1xx_hal::gpio::ExtiPin;

#[derive(Debug, Clone, Copy, PartialEq, Format)]
pub enum Dw1000State {
    Ready,
    Sending,
    Receiving,
}

#[derive(Debug, Clone, Copy, PartialEq, Format)]
pub enum Dw1000MessageType {
    Ping,
    RangingRequest,
    RangingResponse(bool),
    Unknown,
}

pub struct Dw1000Wrapper {
    dw1000_ready: Option<DwTypeReady>,
    dw1000_sending: Option<DwTypeSending>,
    dw1000_receiving: Option<DwTypeReceiving>,
    irq: DwIrqType,
    distance_history: [u64; 10],
}

impl Dw1000Wrapper {
    pub fn new(dw1000: DwTypeReady, irq: DwIrqType) -> Self {
        Dw1000Wrapper {
            dw1000_ready: Some(dw1000),
            dw1000_sending: None,
            dw1000_receiving: None,
            irq,
            distance_history: [0; 10],
        }
    }

    pub fn start_receiving(&mut self) -> Result<(), Error> {
        if let Some(dw1000) = self.dw1000_ready.take() {
            defmt::debug!("Start receiving");

            let receiving = dw1000
                .receive(RxConfig::default())
                .expect("Could not start receiving"); // TODO: Find a way to do proper error handling
            self.dw1000_receiving = Some(receiving);
            Ok(())
        } else if self.dw1000_receiving.is_some() {
            self.finish_receiving()?;
            self.start_receiving()
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn get_state(&self) -> Dw1000State {
        let state = (
            self.dw1000_ready.is_some(),
            self.dw1000_receiving.is_some(),
            self.dw1000_sending.is_some(),
        );

        match state {
            (true, false, false) => Dw1000State::Ready,
            (false, true, false) => Dw1000State::Receiving,
            (false, false, true) => Dw1000State::Sending,
            state => defmt::panic!("Invalid state: {:?}", state),
        }
    }

    pub fn finish_receiving(&mut self) -> Result<(), Error> {
        if let Some(dw1000) = self.dw1000_receiving.take() {
            defmt::debug!("Finish receiving");

            let ready = dw1000.finish_receiving().map_err(|(receiving, e)| {
                self.dw1000_receiving = Some(receiving);
                e
            })?;

            self.dw1000_ready = Some(ready);

            Ok(())
        } else if self.dw1000_ready.is_some() {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn finish_sending(&mut self) -> Result<(), Error> {
        if let Some(dw1000) = self.dw1000_sending.take() {
            defmt::debug!("Finish sending");

            let ready = dw1000.finish_sending().map_err(|(sending, e)| {
                self.dw1000_sending = Some(sending);
                e
            })?;

            self.dw1000_ready = Some(ready);

            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn receive_message(&mut self) -> Result<Dw1000MessageType, Error> {
        let mut delay = get_delay();

        if let Some(mut dw1000) = self.dw1000_receiving.take() {
            let mut buf = [0; 128];

            defmt::debug!("Receive message");

            delay.delay_us(1000u32);

            let mut i = 0;

            let result = loop {
                match dw1000.wait(&mut buf) {
                    Ok(result) => break Ok(result),
                    Err(nb::Error::WouldBlock) => {
                        if i == 2 {
                            break Err(nb::Error::WouldBlock);
                        } else {
                            defmt::warn!("receive_message retry");

                            let sys_status = dw1000.ll().sys_status().read().unwrap();
                            defmt::warn!(
                                "ldedone: {:?}, rxdfr: {:?}, rxfcg: {:?}, rxfce: {}, ldeerr: {:?}",
                                sys_status.ldedone(),
                                sys_status.rxdfr(),
                                sys_status.rxfcg(),
                                sys_status.rxfce(),
                                sys_status.ldeerr()
                            );
                            delay.delay_us(1000u32);
                            i += 1;
                        }
                    }
                    Err(e) => break Err(e),
                }
            };

            let message = match result {
                Ok(message) => message,
                Err(e) => {
                    self.dw1000_receiving = Some(dw1000);
                    return Err(Error::from(e));
                }
            };

            self.dw1000_receiving = Some(dw1000);
            self.finish_receiving()?;

            self.handle_message(message)
        } else {
            Err(Error::InvalidState)
        }
    }

    fn handle_message(&mut self, message: dw1000::hl::Message) -> Result<Dw1000MessageType, Error> {
        if let Some(mut dw1000) = self.dw1000_ready.take() {
            let ping = ranging::Ping::decode::<DwSpiType, DwCsType>(&message);
            let request = ranging::Request::decode::<DwSpiType, DwCsType>(&message);
            let response = ranging::Response::decode::<DwSpiType, DwCsType>(&message);

            if let Ok(Some(ping)) = ping {
                defmt::debug!("Sending ranging request...");

                let result = ranging::Request::new(&mut dw1000, &ping);

                let sending = match result {
                    Ok(message) => message.send(dw1000),
                    Err(e) => {
                        self.dw1000_ready = Some(dw1000);
                        Err(e)
                    }
                }?;

                self.dw1000_sending = Some(sending);
                Ok(Dw1000MessageType::Ping)
            } else if let Ok(Some(request)) = request {
                defmt::debug!("Sending ranging response...");

                let result = ranging::Response::new(&mut dw1000, &request);

                let sending = match result {
                    Ok(message) => message.send(dw1000),
                    Err(e) => {
                        self.dw1000_ready = Some(dw1000);
                        Err(e)
                    }
                }?;

                self.dw1000_sending = Some(sending);
                Ok(Dw1000MessageType::RangingRequest)
            } else if let Ok(Some(response)) = response {
                defmt::debug!("Received ranging response");
                let mut valid = false;

                // If this is not a PAN ID and short address, it doesn't
                // come from a compatible node. Ignore it.
                if let mac::Address::Short(pan_id, addr) = response.source {
                    // Ranging response received. Compute distance.
                    let distance_mm = ranging::compute_distance_mm(&response);

                    match distance_mm {
                        Ok(distance_mm) if distance_mm < 20_000 => {
                            let distance_cm = distance_mm / 10;
                            let corrected_distance = Dw1000Wrapper::correct_distance(distance_cm);
                            defmt::debug!(
                                "{:04x}:{:04x} - {} cm - uncorrected {} cm",
                                pan_id.0,
                                addr.0,
                                corrected_distance as u32,
                                distance_cm
                            );
                            self.update_distance(corrected_distance);
                            valid = true;
                        }
                        Ok(distance_mm) => {
                            defmt::warn!("Computed distance too large: {:?}mm", distance_mm)
                        }
                        Err(_) => {
                            defmt::warn!(
                                "Could not compute distance from {:04x}:{:04x}",
                                pan_id.0,
                                addr.0
                            );
                        }
                    }
                }
                self.dw1000_ready = Some(dw1000);
                Ok(Dw1000MessageType::RangingResponse(valid))
            } else {
                defmt::warn!("Ignoring unknown message");
                self.dw1000_ready = Some(dw1000);
                Ok(Dw1000MessageType::Unknown)
            }
        } else {
            Err(Error::InvalidState)
        }
    }

    /// Simple correction based on https://github.com/braun-embedded/rust-dw1000/issues/105
    ///
    /// <corrected distance> = <measured distance> + <range bias>
    /// <range bias> = <base part> + <distance-dependent part>
    ///
    /// <basepart> = -23 cm // for 16 MHz PRF, narrow-band channel
    ///
    /// Linear Regression:
    ///
    /// <measured distance> <= 1200: (30/1200)*x
    /// <measured distance> >  1200: (6/2500) *x + 27.12
    fn correct_distance(distance_cm: u64) -> u64 {
        let dep_part = if distance_cm <= 1200 {
            (30f32 / 1200f32) * distance_cm as f32
        } else {
            (6f32 / 2500f32) * distance_cm as f32 + 27.12f32
        };
        (distance_cm as f32 - 23f32 + dep_part).max(0f32) as u64
    }

    fn update_distance(&mut self, distance_cm: u64) {
        self.distance_history[..].rotate_right(1);
        self.distance_history[0] = distance_cm;
    }

    pub fn get_average_distance(&self) -> u64 {
        self.distance_history.iter().sum::<u64>() / self.distance_history.len() as u64
    }

    pub fn get_last_distance(&self) -> u64 {
        self.distance_history[0]
    }

    pub fn send_ping(&mut self) -> Result<(), Error> {
        if let Some(mut dw1000) = self.dw1000_ready.take() {
            defmt::debug!("Sending ping...");

            let result = ranging::Ping::new(&mut dw1000);

            let sending = match result {
                Ok(message) => message.send(dw1000),
                Err(e) => {
                    self.dw1000_ready = Some(dw1000);
                    Err(e)
                }
            }?;

            self.dw1000_sending = Some(sending);

            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn handle_interrupt(&mut self) -> Result<(), Error> {
        if self.irq.check_interrupt() {
            self.irq.clear_interrupt_pending_bit();
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }

    pub fn shutdown(&mut self) {
        let _ = self.finish_sending();
        let _ = self.finish_receiving();
    }
}
