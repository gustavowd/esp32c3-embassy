// Copyright Claudio Mattera 2024.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! HTTP client

use embassy_net::dns::DnsSocket;
use embassy_net::dns::Error as DnsError;
use embassy_net::tcp::client::TcpClient;
use embassy_net::tcp::client::TcpClientState;
use embassy_net::tcp::ConnectError as TcpConnectError;
use embassy_net::tcp::Error as TcpError;
use embassy_net::Stack;
use embassy_time::{Duration, WithTimeout};
use log::debug;

use reqwless::client::HttpClient;
use reqwless::client::TlsConfig;
use reqwless::client::TlsVerify;
use reqwless::request::Method;
use reqwless::request::RequestBuilder;
use reqwless::headers::ContentType;
use reqwless::Error as ReqlessError;

use heapless::Vec;

use rand_core::RngCore as _;

use crate::RngWrapper;
//use esp_mbedtls::Tls;

/// Response size
const RESPONSE_SIZE: usize = 4096;

/// HTTP client
///
/// This trait exists to be extended with requests to specific sites, like in
/// [`WorldTimeApiClient`][crate::worldtimeapi::WorldTimeApiClient].
pub trait ClientTrait {
    /// Send an HTTP request
    #[allow(unused, async_fn_in_trait)]
    async fn get_request(&mut self, url: &str, timeout: Duration) -> Result<Vec<u8, RESPONSE_SIZE>, Error>;
    #[allow(unused, async_fn_in_trait)]
    async fn post_request(&mut self, url: &str, ct: ContentType, body: &[u8]) -> Result<Vec<u8, RESPONSE_SIZE>, Error>;
}

/// HTTP client
//pub struct Client<'a,'d> {
pub struct Client<'a> {
    /// Wifi stack
    stack: Stack<'a>,

    /// Random numbers generator
    rng: RngWrapper,

    //tls: esp_mbedtls::Tls<'d>,

    /// TCP client state
    tcp_client_state: TcpClientState<1, 4096, 4096>,

    // Buffer for received TLS data
    read_record_buffer: [u8; 16640],

    // Buffer for transmitted TLS data
    write_record_buffer: [u8; 16640],
}

//impl<'a,'d> Client<'a,'d> {
impl<'a> Client<'a> {
    /// Create a new client
    pub fn new(stack: Stack<'a>, rng: RngWrapper/*tls: esp_mbedtls::Tls<'d>*/) -> Self { //, rng: RngWrapper
        debug!("Create TCP client state");
        let tcp_client_state = TcpClientState::<1, 4096, 4096>::new();

        Self {
            stack,
            rng,
            //tls,
            tcp_client_state,

            read_record_buffer: [0_u8; 16640],
            write_record_buffer: [0_u8; 16640],
        }
    }
}

impl ClientTrait for Client<'static> {
    async fn get_request(&mut self, url: &str, timeout: Duration) -> Result<Vec<u8, RESPONSE_SIZE>, Error> {
        debug!("Send HTTPs request to {url}");

        debug!("Create DNS socket");
        let dns_socket = DnsSocket::new(self.stack);

        let seed = self.rng.next_u64();

        /*
        let tls_config = TlsConfig::new(
            reqwless::TlsVersion::Tls1_3,
            reqwless::Certificates {
                ca_chain: None,
                certificate: None,
                private_key: None,
                password: None,
                //ca_chain: reqwless::X509::pem(concat!(include_str!(".././certs/www.google.com.pem"), "\0").as_bytes()).ok(),
                //certificate: reqwless::X509::pem(concat!(include_str!(".././certs/certificate.pem"), "\0").as_bytes()).ok(),
                //private_key: reqwless::X509::pem(concat!(include_str!(".././certs/private_key.pem"), "\0").as_bytes()).ok(), 
                //..Default::default()
            },
            self.tls.reference(), // Will use hardware acceleration
        );
        */

        let tls_config = TlsConfig::new(
            seed,
            &mut self.read_record_buffer,
            &mut self.write_record_buffer,
            TlsVerify::None,
        );
        
        /*
        let tls_config;
        if let Some(cert) = cert {
            if let Some(key) = key {
                tls_config = TlsConfig::new(
                    seed,
                    &mut self.read_record_buffer,
                    &mut self.write_record_buffer,
                    TlsVerify::Psk { identity: cert, psk: key },
                );
            }else{
                tls_config = TlsConfig::new(
                    seed,
                    &mut self.read_record_buffer,
                    &mut self.write_record_buffer,
                    TlsVerify::None,
                );
            }
        }else{
            tls_config = TlsConfig::new(
                seed,
                &mut self.read_record_buffer,
                &mut self.write_record_buffer,
                TlsVerify::None,
            );
        }
        */
        

        debug!("Create TCP client");
        let tcp_client = TcpClient::new(self.stack, &self.tcp_client_state);

        debug!("Create HTTP client");
        let mut client = HttpClient::new_with_tls(&tcp_client, &dns_socket, tls_config);
        //let mut client = HttpClient::new(&tcp_client, &dns_socket);

        debug!("Create HTTP request");
        let mut buffer = [0_u8; 4096];
        let mut request = client.request(Method::GET, url)
            .with_timeout(timeout)
            .await??;

        debug!("Send HTTP request");
        let response = request.send(&mut buffer).await?;

        debug!("Response status: {:?}", response.status);

        let buffer = response.body().read_to_end().await?;

        debug!("Read {} bytes", buffer.len());

        let output =
            Vec::<u8, RESPONSE_SIZE>::from_slice(buffer).map_err(|()| Error::ResponseTooLarge)?;

        Ok(output)
    }

    async fn post_request(&mut self, url: &str, ct: ContentType, body: &[u8]) -> Result<Vec<u8, RESPONSE_SIZE>, Error> {
        debug!("Send HTTPs request to {url}");

        debug!("Create DNS socket");
        let dns_socket = DnsSocket::new(self.stack);

        let seed = self.rng.next_u64();
        let tls_config = TlsConfig::new(
            seed,
            &mut self.read_record_buffer,
            &mut self.write_record_buffer,
            TlsVerify::None,
        );

        /*
        let tls_config = TlsConfig::new(
            seed,
            &mut self.read_record_buffer,
            &mut self.write_record_buffer,
            TlsVerify::None,
        );
        */

        /*
        let tls_config = TlsConfig::new(
            reqwless::TlsVersion::Tls1_2,
            reqwless::Certificates {
                //ca_chain: reqwless::X509::pem(CERT.as_bytes()).ok(),
                //certificate: reqwless::X509::pem(concat!(include_str!(".././pki/ECC-secp256r1/load_client01.pem"), "\0").as_bytes()).ok(),
                //private_key: reqwless::X509::pem(concat!(include_str!(".././pki/ECC-secp256r1/load_client01.key"), "\0").as_bytes()).ok(),
                //password: None,
                //ca_chain: None,
                //ca_chain: reqwless::X509::pem(concat!(include_str!(".././pki/ECC-secp256r1/ca.pem"), "\0").as_bytes()).ok()
                ..Default::default()
            },
            self.tls.reference(), // Will use hardware acceleration
        );
        */

        debug!("Create TCP client");
        let tcp_client = TcpClient::new(self.stack, &self.tcp_client_state);

        debug!("Create HTTP client");
        let mut client = HttpClient::new_with_tls(&tcp_client, &dns_socket, tls_config);
        //let mut client = HttpClient::new(&tcp_client, &dns_socket);

        debug!("Create HTTP request");
        let mut buffer = [0_u8; 4096];
        let mut request = client
            .request(Method::POST, url)
            .await?
            .body(body)
            .content_type(ct);

        debug!("Send HTTP request");
        let response = request.send(&mut buffer).await.unwrap();

        debug!("Response status: {:?}", response.status);

        let buffer = response.body().read_to_end().await?;

        debug!("Read {} bytes", buffer.len());

        let output =
            Vec::<u8, RESPONSE_SIZE>::from_slice(buffer).map_err(|()| Error::ResponseTooLarge)?;

        Ok(output)
    }
}

/// An error within an HTTP request
#[derive(Debug)]
pub enum Error {
    /// Response was too large
    ResponseTooLarge,

    /// Error within TCP streams
    Tcp(#[allow(unused)] TcpError),

    /// Error within TCP connection
    TcpConnect(#[allow(unused)] TcpConnectError),

    /// Error within DNS system
    Dns(#[allow(unused)] DnsError),

    /// Error in HTTP client
    Reqless(#[allow(unused)] ReqlessError),

    Time(embassy_time::TimeoutError)
}

impl From<embassy_time::TimeoutError> for Error {
    fn from(error: embassy_time::TimeoutError) -> Self {
        Self::Time(error)
    }
}

impl From<TcpError> for Error {
    fn from(error: TcpError) -> Self {
        Self::Tcp(error)
    }
}

impl From<TcpConnectError> for Error {
    fn from(error: TcpConnectError) -> Self {
        Self::TcpConnect(error)
    }
}

impl From<DnsError> for Error {
    fn from(error: DnsError) -> Self {
        Self::Dns(error)
    }
}

impl From<ReqlessError> for Error {
    fn from(error: ReqlessError) -> Self {
        Self::Reqless(error)
    }
}