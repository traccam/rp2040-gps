#![no_std]
#![no_main]

use traccam_common::display::draw_status_display;
use traccam_common::display::DisplayState;
use embedded_graphics::text::Baseline;
use embedded_graphics::prelude::{DrawTarget, Point};
use embedded_graphics::text::Text;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10, FONT_7X13};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use cortex_m::prelude::_embedded_hal_serial_Read;
use embedded_graphics::Drawable;
use defmt::*;
use ssd1306::prelude::*;
use defmt_rtt as _;
use panic_probe as _;

use embedded_io_async::Read;
use embassy_executor::Spawner;
use embassy_rp::{bind_interrupts, i2c};
use embassy_rp::i2c::{Async, I2c};
use embassy_rp::peripherals::{I2C1, UART0};
use embassy_rp::uart::{BufferedUartRx, BufferedInterruptHandler};
use embassy_rp::uart;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use heapless::{format, String};
use nmea::Nmea;
use core::fmt::Write;
use embassy_sync::signal::Signal;

bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
    I2C1_IRQ => i2c::InterruptHandler<I2C1>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut i2cc = i2c::Config::default();
    i2cc.frequency = 400_000;
    let t = do_display(I2c::new_async(p.I2C1, p.PIN_27, p.PIN_26, Irqs, i2cc));
    spawner.spawn(t).unwrap();

    let mut config = uart::Config::default();
    config.baudrate = 115200;

    let mut rx_buf = [0u8; 4096];

    let mut uart_rx = BufferedUartRx::new(p.UART0, Irqs, p.PIN_1, &mut rx_buf, config);

    let mut nmea = Nmea::default();
    let mut buffer: String<128> = String::new();

    info!("Waiting for u-blox M10 GPS data at 38400 baud...");

    loop {
        let mut byte = [0u8; 1];

        match embedded_io_async::Read::read(&mut uart_rx, &mut byte).await {
            Ok(b) => {
                let b = byte[0] as char;

                if b == '\n' {
                    if let Ok(mtype) = nmea.parse(buffer.as_str()) {
                        let mut state = DisplayState::default();
                        let sats = nmea.satellites();
                        let max_snr = sats.iter()
                            .filter_map(|s| s.snr())
                            .max_by(|l,r|l.total_cmp(r)).unwrap_or(0.0);
                        state.sats = sats.len() as _;
                        info!("{} Fix: {} Sats: {} PNR: {}", mtype, nmea.fix_type, sats.len(), nmea.pdop.unwrap_or(0.0));

                        // Checking only for latitude and longitude
                        if let (Some(lat), Some(lon)) = (nmea.latitude, nmea.longitude) {
                            info!("Lat: {} | Lon: {}", lat, lon);
                            state.lat = lat;
                            state.lon = lon;
                        }

                        if let Some(time) = nmea.fix_timestamp() {
                            state.time = time;
                        }

                        DISPLAY_SIGNAL.signal(state);
                    }
                    buffer.clear();
                } else if b != '\r' {
                    let _ = buffer.push(b);
                }
            }
            Err(e) => {
                error!("UART Read Error: {:?}", e);
            }
        }
    }
}

static DISPLAY_SIGNAL: Signal<CriticalSectionRawMutex, DisplayState> = Signal::new();
#[embassy_executor::task]
async fn do_display(i2c: I2c<'static, I2C1, Async>) {
    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306::new(
        interface,
        DisplaySize128x32,
        DisplayRotation::Rotate0,
    ).into_buffered_graphics_mode();
    display.init().unwrap();

    let mut state = DisplayState::default();
    loop {
        display.clear(BinaryColor::Off).unwrap();

        if DISPLAY_SIGNAL.signaled() {
            let d = DISPLAY_SIGNAL.wait().await;
            state = d;
        };

        draw_status_display(&mut display, &state);

        display.flush().unwrap();
        Timer::after_millis(40).await;
    }
}