use crate::types::{DwCsType, DwSpiType};
use defmt::Format;

#[derive(Debug, Clone, Copy, PartialEq, Format)]
pub enum Error {
    Interface,
    DW1000(u8),
    InvalidState,
    WouldBlock,
}

impl From<dw1000::Error<DwSpiType, DwCsType>> for Error {
    fn from(e: dw1000::Error<DwSpiType, DwCsType>) -> Self {
        match e {
            dw1000::Error::Spi(_) => Error::Interface,
            dw1000::Error::Fcs => Error::DW1000(0),
            dw1000::Error::Phy => Error::DW1000(1),
            dw1000::Error::BufferTooSmall { .. } => Error::DW1000(2),
            dw1000::Error::ReedSolomon => Error::DW1000(3),
            dw1000::Error::FrameWaitTimeout => Error::DW1000(4),
            dw1000::Error::Overrun => Error::DW1000(5),
            dw1000::Error::PreambleDetectionTimeout => Error::DW1000(6),
            dw1000::Error::SfdTimeout => Error::DW1000(7),
            dw1000::Error::FrameFilteringRejection => Error::DW1000(8),
            dw1000::Error::Frame(_) => Error::DW1000(9),
            dw1000::Error::DelayedSendTooLate => Error::DW1000(10),
            dw1000::Error::DelayedSendPowerUpWarning => Error::DW1000(11),
            dw1000::Error::Ssmarshal(_) => Error::DW1000(12),
            dw1000::Error::InvalidConfiguration => Error::DW1000(13),
        }
    }
}

impl From<nb::Error<dw1000::Error<DwSpiType, DwCsType>>> for Error {
    fn from(e: nb::Error<dw1000::Error<DwSpiType, DwCsType>>) -> Self {
        match e {
            nb::Error::Other(e) => e.into(),
            nb::Error::WouldBlock => Error::WouldBlock,
        }
    }
}
