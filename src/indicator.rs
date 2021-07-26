use crate::types::WsType;
use defmt::Format;
use smart_leds::{SmartLedsWrite, RGB8};

#[derive(Debug, Clone, Copy, PartialEq, Format)]
pub enum DistanceRange {
    OutOfRange,
    Long,
    OkLong,
    Ok,
    OkShort,
    Short,
}

pub trait DistanceIndicator {
    type Error;

    fn update_range(
        &mut self,
        current_distance: u64,
        target_distance: u64,
        tolerance: u64,
    ) -> Result<DistanceRange, Self::Error>;
    fn set_out_of_range(&mut self) {
        self.set_range(DistanceRange::OutOfRange);
    }
    fn get_range(&self) -> DistanceRange;
    fn set_range(&mut self, range: DistanceRange);
    fn shutdown(&mut self);
}

pub struct LedIndicator {
    ws: WsType,
    range: DistanceRange,
}

impl LedIndicator {
    pub fn new(mut ws: WsType) -> Self {
        let data: [RGB8; 5] = [RGB8::default(); 5];
        ws.write(data.iter().cloned()).unwrap();

        LedIndicator {
            ws,
            range: DistanceRange::OutOfRange,
        }
    }

    fn update_leds(&mut self) {
        let mut data: [RGB8; 5] = [RGB8::default(); 5];

        let red = RGB8::new(0xff, 0, 0);
        let green = RGB8::new(0, 0xff, 0);
        let blue = RGB8::new(0, 0, 0xff);

        match self.range {
            DistanceRange::OutOfRange => {}
            DistanceRange::Long => {
                data[3] = blue;
                data[4] = blue;
            }
            DistanceRange::OkLong => {
                data[2] = green;
                data[3] = blue;
            }
            DistanceRange::Ok => {
                data[2] = green;
            }
            DistanceRange::OkShort => {
                data[1] = red;
                data[2] = green;
            }
            DistanceRange::Short => {
                data[0] = red;
                data[1] = red;
            }
        }

        self.ws.write(data.iter().cloned()).unwrap();
    }
}

impl DistanceIndicator for LedIndicator {
    type Error = ();

    fn update_range(
        &mut self,
        current_distance: u64,
        target_distance: u64,
        tolerance: u64,
    ) -> Result<DistanceRange, Self::Error> {
        let range = match current_distance {
            d if d < target_distance - 2 * tolerance => DistanceRange::Short,
            d if d < target_distance - tolerance => DistanceRange::OkShort,
            d if d > target_distance + tolerance => DistanceRange::OkLong,
            d if d > target_distance + 2 * tolerance => DistanceRange::Long,
            _ => DistanceRange::Ok,
        };

        if range != self.range {
            self.range = range;
            self.update_leds();
        }

        Ok(range)
    }

    fn set_out_of_range(&mut self) {
        self.set_range(DistanceRange::OutOfRange);
        self.update_leds();
    }

    fn get_range(&self) -> DistanceRange {
        self.range
    }

    fn set_range(&mut self, range: DistanceRange) {
        self.range = range
    }

    fn shutdown(&mut self) {
        let mut data: [RGB8; 5] = [RGB8::default(); 5];

        let red = RGB8::new(10, 0, 0);
        data[0] = red;
        data[4] = red;

        self.ws.write(data.iter().cloned()).unwrap();
    }
}
