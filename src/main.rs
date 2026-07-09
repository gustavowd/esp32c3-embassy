//! Embassy DHCP Example
//!
//!
//! Set SSID and PASSWORD env variable before running this example.
//!
//! This gets an ip address via DHCP then performs an HTTP get request to some "random" server
//!
//! Because of the huge task-arena size configured this won't work on ESP32-S2

//% FEATURES: embassy embassy-generic-timers esp-wifi esp-wifi/async esp-wifi/embassy-net esp-wifi/wifi-default esp-wifi/wifi esp-wifi/utils
//% CHIPS: esp32 esp32s2 esp32s3 esp32c2 esp32c3 esp32c6

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;


//mod random;
//use self::random::RngWrapper;

//mod clock;
//use self::clock::Clock;
//use self::clock::Error as ClockError;

//mod worldtimeapi;

use embassy_net::{
    Runner,
    StackResources,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{
    Async,
    clock::CpuClock,
    interrupt::software::SoftwareInterruptControl,
    ram,
    rng::Rng,
    time::Rate,
    timer::timg::TimerGroup,
    i2c::master::{I2c, Config as MasterConfig}
};

use esp_println::println;

use esp_radio::wifi::{
    Config,
    ControllerConfig,
    Interface,
    WifiController,
    scan::ScanConfig,
    sta::StationConfig,
};

use reqwless::{
    client::HttpClient,
    request::{Method, RequestBuilder},
};

//use esp_mbedtls::Tls;

//use ieee2030_5_no_std_lib::http::Client as HttpClient;
//use ieee2030_5_no_std_lib::random::RngWrapper;
//use ieee2030_5_no_std_lib::ieee2030_5::IEEE20305;

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
esp_bootloader_esp_idf::esp_app_desc!();

//#[cfg(feature = "alloc-hooks")]
use enumset::EnumSet;
//#[cfg(feature = "alloc-hooks")]
use esp_alloc::{EspHeap, MemoryCapability};

//#[cfg(feature = "alloc-hooks")]
#[unsafe(no_mangle)]
unsafe extern "Rust" fn _esp_alloc_alloc(
    _heap: &EspHeap,
    _caps: EnumSet<MemoryCapability>,
    ptr: usize,
    size: usize,
) {
    println!("Allocated {} bytes: {:x}", size, ptr);
}

//#[cfg(feature = "alloc-hooks")]
#[unsafe(no_mangle)]
unsafe extern "Rust" fn _esp_alloc_dealloc(_heap: &EspHeap, ptr: usize, size: usize) {
    println!("Deallocated {} bytes: {:x}", size, ptr);
}

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write($val);
        x
    }};
}

const SSID: &str = "SE28CP";
const PASSWORD: &str = "12345678";
//const SSID: &str = "UTFPR-SERVIDOR";
//const USERNAME: &str = "gustavo";
//const PASSWORD: &str = "12345678";

// O tipo do driver I2C utiliza a assinatura: I2c<'static, Async>
#[embassy_executor::task]
async fn i2c_worker_task(mut i2c: I2c<'static, Async>) {
    //info!("Tarefa I2C iniciada com sucesso!");
    
    let mut buffer = [0u8; 2];
    let device_address = 0x55;

    loop {
        // Exemplo de leitura assíncrona periódica dentro da task
        // Substitua pelo registrador correto do seu sensor
        if let Err(err) = i2c.write_read_async(device_address, &[0x00], &mut buffer).await {
            log::error!("Erro de comunicação I2C: {:?}", err);
        } else {
            //info!("Dados lidos do I2C: {:?}", buffer);
        }

        // Aguarda 1 segundo antes da próxima leitura
        embassy_time::Timer::after_secs(1).await;
    }
}

#[embassy_executor::task]
async fn run() {
    loop {
        esp_println::println!("Hello world from embassy using esp-hal-async!");
        Timer::after(Duration::from_millis(1_000)).await;
    }
}

#[esp_hal::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[ram(reclaimed)] size: 64 * 1024);
    esp_alloc::heap_allocator!(size: 36 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_password(PASSWORD.into()),
    );

    println!("Starting wifi");
    let wifi_interface = esp_radio::wifi::Interface::station();
    let mut controller = esp_radio::wifi::WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .unwrap();
    println!("Wifi configured and started!");

    let config = embassy_net::Config::dhcpv4(Default::default());

    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    // Init network stack
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        mk_static!(StackResources<3>, StackResources::<3>::new()),
        seed,
    );

    println!("Scan");
    let scan_config = ScanConfig::default().with_max(10);
    let result = controller.scan_async(&scan_config).await.unwrap();
    for ap in result {
        println!("{:?}", ap);
    }

    spawner.spawn(connection(controller).unwrap());
    spawner.spawn(net_task(runner).unwrap());

    stack.wait_config_up().await;

    if let Some(config) = stack.config_v4() {
        println!("Got IP: {}", config.address);
    }

    // Init HTTP client
    let tcp_client = TcpClient::new(
        stack,
        mk_static!(
            TcpClientState<1, 1500, 1500>,
            TcpClientState::<1, 1500, 1500>::new()
        ),
    );
    let dns_client = DnsSocket::new(stack);

    esp_println::println!("Initializing I2C Slave on I2C0...");

    // Configure SDA and SCL pins
    let sda = peripherals.GPIO2;
    let scl = peripherals.GPIO3;

    // Create a new I2C master instance with default configuration and the specified SDA and SCL pins
    let config = MasterConfig::default().with_frequency(Rate::from_khz(400));   // Set I2C frequency to 400 kHz
    let i2c_master = I2c::new(peripherals.I2C0, config)
        .unwrap()
        .with_sda(sda)
        .with_scl(scl)
        .into_async();

    // Dispara (spawn) a tarefa passando o driver por parâmetro
    spawner.spawn(i2c_worker_task(i2c_master).unwrap());

    loop {
        Timer::after(Duration::from_millis(1000)).await;

        let mut client = HttpClient::new(&tcp_client, &dns_client);
        let mut rx_buf = [0u8; 4096];

        let builder = client
            .request(Method::GET, "http://httpbin.org/get?hello=Hello+esp-hal")
            .await
            .unwrap();

        let mut builder = builder.headers(&[("Host", "httpbin.org"), ("Connection", "close")]);

        let response = builder.send(&mut rx_buf).await.unwrap();

        match response.body().read_to_end().await {
            Ok(data) => {
                if let Ok(st) = core::str::from_utf8(data) {
                    println!("Body: {}", st);
                }
            }
            Err(e) => println!("Body error: {:?}", e),
        }
        Timer::after(Duration::from_millis(3000)).await;
    }
}


#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");

    loop {
        println!("About to connect...");

        match controller.connect_async().await {
            Ok(info) => {
                println!("Wifi connected to {:?}", info);

                // wait until we're no longer connected
                let info = controller.wait_for_disconnect_async().await.ok();
                println!("Disconnected: {:?}", info);
            }
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
            }
        }

        Timer::after(Duration::from_millis(5000)).await
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}