use dw1000::{Ready, Receiving, Sending, DW1000};
use stm32f1xx_hal::gpio::gpioa::{PA4, PA5, PA6, PA7};
use stm32f1xx_hal::gpio::gpiob::PB0;
use stm32f1xx_hal::gpio::{Alternate, Floating, Input, Output, PushPull};
use stm32f1xx_hal::spi::Spi1NoRemap;

pub type DwSpiType = stm32f1xx_hal::spi::Spi<
    stm32f1xx_hal::pac::SPI1,
    Spi1NoRemap,
    (
        PA5<Alternate<PushPull>>,
        PA6<Input<Floating>>,
        PA7<Alternate<PushPull>>,
    ),
    u8,
>;

pub type DwCsType = PA4<Output<PushPull>>;

pub type DwType<STATE> = DW1000<DwSpiType, DwCsType, STATE>;

pub type DwTypeReady = DW1000<DwSpiType, DwCsType, Ready>;
pub type DwTypeSending = DW1000<DwSpiType, DwCsType, Sending>;
pub type DwTypeReceiving = DW1000<DwSpiType, DwCsType, Receiving>;

pub type DwIrqType = PB0<Input<Floating>>;
