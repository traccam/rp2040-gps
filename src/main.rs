#![no_std]
#![no_main]

use cortex_m::prelude::_embedded_hal_serial_Read;
use defmt::*;
use defmt_rtt as _;
use panic_probe as _;

use embedded_io_async::Read;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::UART0;
// NOTICE: We imported BufferedInterruptHandler here instead of InterruptHandler
use embassy_rp::uart::{BufferedUartRx, Config, BufferedInterruptHandler};
use heapless::String;
use nmea::Nmea;

// Bind the UART0 interrupt to Embassy's internal buffered handler
bind_interrupts!(struct Irqs {
    UART0_IRQ => BufferedInterruptHandler<UART0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // Set to 38400 baud as per the seller's specs
    let mut config = Config::default();
    config.baudrate = 115200;

    // Create a backing array for the interrupt ring buffer
    let mut rx_buf = [0u8; 512];

    // Initialize BufferedUartRx
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
                    if let Ok(_) = nmea.parse(buffer.as_str()) {
                        info!("Fix: {} Sats: {}", nmea.fix_type, nmea.satellites().len());

                        // Checking only for latitude and longitude
                        if let (Some(lat), Some(lon)) = (nmea.latitude, nmea.longitude) {
                            info!("Lat: {} | Lon: {}", lat, lon);
                        } else {
                            debug!("Tracking... waiting for fix.");
                        }

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