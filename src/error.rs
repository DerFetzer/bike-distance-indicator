use crate::types::{DwCsType, DwSpiType};
use defmt::Format;

#[derive(Debug, Clone, Copy, PartialEq, Format)]
pub enum Error {
    Interface,
    DW1000,
    InvalidState,
    WouldBlock,
}

impl From<dw1000::Error<DwSpiType, DwCsType>> for Error {
    fn from(e: dw1000::Error<DwSpiType, DwCsType>) -> Self {
        match e {
            dw1000::Error::Spi(_) => Error::Interface,
            _ => Error::DW1000,
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
