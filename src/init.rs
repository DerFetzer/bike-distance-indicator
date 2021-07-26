use crate::battery::BatteryMonitor;
use crate::helper::get_delay;
use crate::indicator::LedIndicator;
use crate::types::{DwIrqType, DwTypeReady, Led1Type};
use dw1000::DW1000;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use embedded_hal::spi::MODE_0;
use stm32f1xx_hal::adc::Adc;
use stm32f1xx_hal::pac::SPI1;
use stm32f1xx_hal::spi::Spi;
use stm32f1xx_hal::{gpio::*, pac::Peripherals, prelude::*};
use ws2812_spi::{Ws2812, MODE as WS_MODE};

pub fn init_hardware(
    mut dp: Peripherals,
    _cp: rtic::Peripherals,
) -> (
    DwTypeReady,
    DwIrqType,
    Led1Type,
    LedIndicator,
    BatteryMonitor,
) {
    defmt::info!("Init hardware");

    // Workaround for probe-run wfi issue
    dp.DBGMCU.cr.write(|w| w.dbg_sleep().set_bit());
    dp.RCC.ahbenr.write(|w| w.dma1en().set_bit());

    let mut delay = get_delay();

    // Take ownership over the raw flash and rcc devices and convert them into the corresponding
    // HAL structs
    let mut flash = dp.FLASH.constrain();
    let mut rcc = dp.RCC.constrain();

    defmt::info!("Init clocks");

    // Freeze the configuration of all the clocks in the system and store the frozen frequencies in
    // `clocks`
    let clocks = rcc
        .cfgr
        .use_hse(16.mhz())
        .sysclk(64.mhz())
        .pclk1(32.mhz())
        .pclk2(64.mhz())
        .freeze(&mut flash.acr);

    let mut afio = dp.AFIO.constrain(&mut rcc.apb2);

    // Acquire the GPIOA peripheral
    let mut gpioa = dp.GPIOA.split(&mut rcc.apb2);
    let mut gpiob = dp.GPIOB.split(&mut rcc.apb2);

    defmt::info!("Init pins");

    let rst_inp = gpiob.pb12.into_floating_input(&mut gpiob.crh);

    while rst_inp.is_low().unwrap() {}
    delay.delay_ms(10u32);

    let mut rst = rst_inp.into_open_drain_output_with_state(&mut gpiob.crh, State::Low);

    let spi_pins = (
        gpioa.pa5.into_alternate_push_pull(&mut gpioa.crl),
        gpioa.pa6.into_floating_input(&mut gpioa.crl),
        gpioa.pa7.into_alternate_push_pull(&mut gpioa.crl),
    );

    let spi_cs = gpioa.pa4.into_push_pull_output(&mut gpioa.crl);

    let mut irq = gpiob.pb0.into_floating_input(&mut gpiob.crl);

    let ws_pins = (
        gpiob.pb13.into_alternate_push_pull(&mut gpiob.crh),
        gpiob.pb14.into_floating_input(&mut gpiob.crh),
        gpiob.pb15.into_alternate_push_pull(&mut gpiob.crh),
    );

    irq.make_interrupt_source(&mut afio);
    irq.enable_interrupt(&mut dp.EXTI);
    irq.trigger_on_edge(&mut dp.EXTI, Edge::RISING);

    let led1 = gpioa
        .pa2
        .into_push_pull_output_with_state(&mut gpioa.crl, State::Low);

    let bat_pin = gpioa.pa3.into_analog(&mut gpioa.crl);

    defmt::info!("Init ADC");

    let adc = Adc::adc1(dp.ADC1, &mut rcc.apb2, clocks);

    defmt::info!("Init SPI");

    let spi1 = Spi::spi1(
        dp.SPI1,
        spi_pins,
        &mut afio.mapr,
        MODE_0,
        2.mhz(), // Clock speed in INIT below 3MHz
        clocks,
        &mut rcc.apb2,
    );

    let spi2 = Spi::spi2(dp.SPI2, ws_pins, WS_MODE, 3.mhz(), clocks, &mut rcc.apb1);

    defmt::info!("Init WS2812");

    let ws = Ws2812::new(spi2);

    defmt::info!("Init Indicator");

    let led_indicator = LedIndicator::new(ws);

    defmt::info!("Init battery monitor");

    let battery_monitor = BatteryMonitor::new(adc, bat_pin);

    defmt::info!("Init DW1000");

    delay.delay_ms(10u32); // Reset
    rst.set_high().unwrap();

    let mut dw1000 = DW1000::new(spi1, spi_cs).init().unwrap();

    // Increase clock speed after INIT
    unsafe {
        (*SPI1::ptr()).cr1.modify(|_, w| w.br().div4());
    }

    dw1000.configure_leds(true, true, true, true, 5).unwrap();

    dw1000
        .enable_tx_interrupts()
        .expect("Failed to enable TX interrupts");
    dw1000
        .enable_rx_interrupts()
        .expect("Failed to enable RX interrupts");

    // These are the hardcoded calibration values from the dwm1001-examples
    // repository[1]. Ideally, the calibration values would be determined using
    // the proper calibration procedure, but hopefully those are good enough for
    // now.
    //
    // [1] https://github.com/Decawave/dwm1001-examples
    dw1000
        .set_antenna_delay(16456, 16300)
        .expect("Failed to set antenna delay");

    defmt::info!("Init hardware finished");

    (dw1000, irq, led1, led_indicator, battery_monitor)
}
