use dw1000::{Ready, Receiving, Sending, DW1000};
use stm32f1xx_hal::adc::Adc;
use stm32f1xx_hal::gpio::gpioa::{PA2, PA3, PA4, PA5, PA6, PA7};
use stm32f1xx_hal::gpio::gpiob::{PB0, PB13, PB14, PB15};
use stm32f1xx_hal::gpio::{Alternate, Analog, Floating, Input, Output, PushPull};
use stm32f1xx_hal::pac::ADC1;
use stm32f1xx_hal::spi::{Spi1NoRemap, Spi2NoRemap};

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

pub type Led1Type = PA2<Output<PushPull>>;
pub type WsType = ws2812_spi::Ws2812<
    stm32f1xx_hal::spi::Spi<
        stm32f1xx_hal::pac::SPI2,
        Spi2NoRemap,
        (
            PB13<Alternate<PushPull>>,
            PB14<Input<Floating>>,
            PB15<Alternate<PushPull>>,
        ),
        u8,
    >,
>;

pub type BatteryAdcType = Adc<ADC1>;
pub type BatteryChType = PA3<Analog>;
