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
use http::ClientTrait;

mod http;
use self::http::Client as HttpClient;

mod random;
use self::random::RngWrapper;

mod clock;
use self::clock::Clock;
//use self::clock::Error as ClockError;

mod worldtimeapi;

use embassy_net::{tcp::TcpSocket, Ipv4Address, StackResources};
use esp_alloc as _;
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, rng::Rng, timer::timg::TimerGroup};
use esp_println::println;
use esp_wifi::{
    init,
    wifi::{
        ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState
        //AuthMethod, EapClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice, WifiState
    },
    EspWifiController
};

//use esp_mbedtls::Tls;

//use ieee2030_5_no_std_lib::http::Client as HttpClient;
//use ieee2030_5_no_std_lib::random::RngWrapper;
//use ieee2030_5_no_std_lib::ieee2030_5::IEEE20305;

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

const SSID: &str = "SE28CP";
const PASSWORD: &str = "12345678";
//const SSID: &str = "UTFPR-SERVIDOR";
//const USERNAME: &str = "gustavo";
//const PASSWORD: &str = "12345678";

#[embassy_executor::task]
async fn run() {
    loop {
        esp_println::println!("Hello world from embassy using esp-hal-async!");
        Timer::after(Duration::from_millis(1_000)).await;
    }
}

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(size: 90 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let rng = Rng::new(peripherals.RNG);
    //let mut tls = Tls::new(peripherals.SHA)
    //    .unwrap()
    //    .with_hardware_rsa(peripherals.RSA);
    //tls.set_debug(5);

    let esp_wifi_ctrl = &*mk_static!(
        EspWifiController<'static>,
        init(timg0.timer0, rng, peripherals.RADIO_CLK).unwrap()
    );

    let (controller, interfaces) = esp_wifi::wifi::new(esp_wifi_ctrl, peripherals.WIFI).unwrap();
    let wifi_interface = interfaces.sta;

    use esp_hal::timer::systimer::SystemTimer;
    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    esp_hal_embassy::init(systimer.alarm0);

    let config = embassy_net::Config::dhcpv4(Default::default());

    let seed = 1234; // very random, very secure seed

    // Init network stack
    let (stack, runner) = embassy_net::new(wifi_interface, config, mk_static!(StackResources<3>, StackResources::<3>::new()), seed);

    spawner.spawn(connection(controller)).ok();
    spawner.spawn(net_task(runner)).ok();
    //spawner.spawn(run()).ok();

    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];

    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    println!("Waiting to get IP address...");
    loop {
        if let Some(config) = stack.config_v4() {
            println!("Got IP: {}", config.address);
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

    //let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    //let mut http_client = HttpClient::new(stack, tls);
    let mut http_client = HttpClient::new(stack, RngWrapper::from(rng));

    println!("Synchronize clock from server");
    if let Ok(clock) = Clock::from_server(&mut http_client, Duration::from_secs(5)).await {
        println!("Clock: {:?}", clock);
    }else{
        println!("Failed to synchronize clock");
    }

    /*
    let url = "https://worldtimeapi.org/api/timezone/America/Sao_Paulo.txt";
    //let url = "https://www.google.com";

    loop {
        match http_client.get_request(url, Duration::from_secs(5)).await {
            Ok(resp) => {
                if let Ok(ret) = heapless::String::from_utf8(resp){
                    println!("Response: {}", ret);
                }
                break;
            },
            Err(err) => println!("Response timeout: {:?}", err)
        }
    }
    */

    //ieee2030_5_no_std_lib::ieee2030_5::set_server_url("https://ecee.pb.utfpr.edu.br:8443");
    //let teste = http_client.get_dcap().await;
    //println!("Dcap {:?}", teste);
    /*
    let data = br#"{"username":"Marcel","password":"supersecret","this is a":"test"}"#;
    let url2 = "https://httpdump.app/dumps/ddc0c046-5b12-4742-b757-5a7dcc11d65d";
    match http_client.post_request(url2,reqwless::headers::ContentType::TextPlain, data).await {
        Ok(response) => {
            let ret = heapless::String::from_utf8(response).unwrap();
            println!("Response: {}", ret);
        },
        Err(err) => println!("Error: {:?}", err)
    }
    */


    loop {
        Timer::after(Duration::from_millis(1_000)).await;

        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        let remote_endpoint = (Ipv4Address::new(142, 250, 185, 115), 80);
        println!("connecting...");
        let r = socket.connect(remote_endpoint).await;
        if let Err(e) = r {
            println!("connect error: {:?}", e);
            continue;
        }
        println!("connected!");
        let mut buf = [0; 1024];
        loop {
            use embedded_io_async::Write;
            let r = socket
                .write_all(b"GET / HTTP/1.0\r\nHost: www.mobile-j.de\r\n\r\n")
                .await;
            if let Err(e) = r {
                println!("write error: {:?}", e);
                break;
            }
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    println!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    println!("read error: {:?}", e);
                    break;
                }
            };
            println!("{}", core::str::from_utf8(&buf[..n]).unwrap());
        }
        Timer::after(Duration::from_millis(10000)).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    println!("start connection task");
    println!("Device capabilities: {:?}", controller.capabilities());
    loop {
        if esp_wifi::wifi::wifi_state() == WifiState::StaConnected {
                // wait until we're no longer connected
                controller.wait_for_event(WifiEvent::StaDisconnected).await;
                Timer::after(Duration::from_millis(5000)).await
        }

        if !matches!(controller.is_started(), Ok(true)) {
            /*
            let client_config = Configuration::EapClient( EapClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                identity: Some(USERNAME.try_into().unwrap()),
                username: Some(USERNAME.try_into().unwrap()),
                password: Some(PASSWORD.try_into().unwrap()),
                auth_method: AuthMethod::WPA2Enterprise,
                ..Default::default()
            });
            */
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: SSID.try_into().unwrap(),
                password: PASSWORD.try_into().unwrap(),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            println!("Starting wifi");
            controller.start_async().await.unwrap();
            println!("Wifi started!");
        }
        println!("About to connect...");

        match controller.connect_async().await {
            Ok(_) => println!("Wifi connected!"),
            Err(e) => {
                println!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(10000)).await
            }
        }
    }
}


#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}