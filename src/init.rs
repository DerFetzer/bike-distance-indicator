use crate::helper::get_delay;
use crate::types::{DwIrqType, DwTypeReady};
use dw1000::DW1000;
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::spi::MODE_0;
use stm32f1xx_hal::spi::Spi;
use stm32f1xx_hal::{gpio::*, pac::Peripherals, prelude::*};

pub fn init_hardware(mut dp: Peripherals, _cp: rtic::Peripherals) -> (DwTypeReady, DwIrqType) {
    defmt::info!("Init hardware");

    // Workaround for probe-run wfi issue
    dp.DBGMCU.cr.write(|w| w.dbg_sleep().set_bit());
    dp.RCC.ahbenr.write(|w| w.dma1en().set_bit());

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

    let mut rst = gpiob
        .pb12
        .into_open_drain_output_with_state(&mut gpiob.crh, State::Low);

    let spi_pins = (
        gpioa.pa5.into_alternate_push_pull(&mut gpioa.crl),
        gpioa.pa6.into_floating_input(&mut gpioa.crl),
        gpioa.pa7.into_alternate_push_pull(&mut gpioa.crl),
    );

    let spi_cs = gpioa.pa4.into_push_pull_output(&mut gpioa.crl);

    let mut irq = gpiob.pb0.into_floating_input(&mut gpiob.crl);

    irq.make_interrupt_source(&mut afio);
    irq.enable_interrupt(&mut dp.EXTI);
    irq.trigger_on_edge(&mut dp.EXTI, Edge::RISING);

    defmt::info!("Init spi");

    let spi = Spi::spi1(
        dp.SPI1,
        spi_pins,
        &mut afio.mapr,
        MODE_0,
        2.mhz(),
        clocks,
        &mut rcc.apb2,
    );

    defmt::info!("Init DW1000");

    let mut delay = get_delay();

    delay.delay_ms(2u32); // Reset
    rst.set_high().unwrap();

    let mut dw1000 = DW1000::new(spi, spi_cs).init().unwrap();

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

    (dw1000, irq)
}
