// Copyright 2025 TiDB Cloud
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    path::Path,
    time::Duration,
};

use log::info;
use reqwest::{Body, Client as HttpClient, StatusCode};
use tokio::io::AsyncRead;
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use crate::error::{Error, Result};

pub type Reader = Box<dyn AsyncRead + Send + Sync + Unpin + 'static>;

#[derive(Debug, Clone, Copy)]
pub enum PresignMode {
    Auto,
    Detect,
    On,
    Off,
}

pub struct PresignedResponse {
    pub method: String,
    pub headers: BTreeMap<String, String>,
    pub url: String,
}

/// Storage requests never share warehouse cookies, authentication or proxy settings.
#[derive(Debug, Clone, Default)]
pub struct PresignOptions {
    pub tls_ca_file: Option<String>,
    /// Explicit opt-out for customer-operated private storage, including local MinIO.
    pub allow_private: bool,
}

fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_broadcast()
                && !ip.is_documentation()
                && !ip.is_multicast()
                && o[0] != 0
                && o[0] < 224
                && !(o[0] == 100 && (64..=127).contains(&o[1]))
                && !(o[0] == 198 && (18..=19).contains(&o[1]))
                && !(o[0] == 192 && o[1] == 0 && o[2] == 0)
                && !(o[0] == 192 && o[1] == 88 && o[2] == 99)
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] < 0x200 || s[1] == 0xdb8))
                && s[0] != 0x2002
                && ((s[0] & 0xfff0) != 0x3ff0)
        }
    }
}

impl PresignOptions {
    async fn client(&self, presigned: &PresignedResponse, method: &str) -> Result<HttpClient> {
        let invalid = || Error::BadArgument("Unsafe presigned storage URL or method".into());
        let url = reqwest::Url::parse(&presigned.url).map_err(|_| invalid())?;
        if presigned.method != method
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.host_str().is_none()
            || (url.scheme() != "https" && !(self.allow_private && url.scheme() == "http"))
        {
            return Err(invalid());
        }
        let host = url
            .host_str()
            .ok_or_else(invalid)?
            .trim_start_matches('[')
            .trim_end_matches(']');
        let port = url.port_or_known_default().ok_or_else(invalid)?;
        let addresses: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
            vec![SocketAddr::new(ip, port)]
        } else {
            tokio::net::lookup_host((host, port))
                .await
                .map_err(|_| Error::IO("Storage DNS lookup failed".into()))?
                .collect()
        };
        if addresses.is_empty()
            || (!self.allow_private && addresses.iter().any(|a| !public_address(a.ip())))
        {
            return Err(invalid());
        }
        // Pin the addresses we checked, so a later DNS change cannot reach a private service.
        let mut builder = HttpClient::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(30))
            .timeout(Duration::from_secs(900))
            .resolve_to_addrs(host, &addresses);
        #[cfg(any(feature = "rustls", feature = "native-tls"))]
        if let Some(file) = &self.tls_ca_file {
            let pem = tokio::fs::read(file).await?;
            builder = builder.add_root_certificate(reqwest::Certificate::from_pem(&pem)?);
        }
        builder.build().map_err(Into::into)
    }
}

pub async fn presign_upload_to_stage(
    presigned: PresignedResponse,
    data: Reader,
    size: u64,
) -> Result<()> {
    presign_upload_with_options(presigned, data, size, &PresignOptions::default()).await
}

pub async fn presign_upload_with_options(
    presigned: PresignedResponse,
    data: Reader,
    size: u64,
    options: &PresignOptions,
) -> Result<()> {
    info!("upload to stage with presigned url, size: {size}");
    let client = options.client(&presigned, "PUT").await?;
    let mut builder = client.put(presigned.url);
    for (k, v) in presigned.headers {
        if k.to_lowercase() == "content-length" {
            continue;
        }
        builder = builder.header(k, v);
    }
    builder = builder.header("Content-Length", size.to_string());
    let stream = Body::wrap_stream(ReaderStream::new(data));
    let resp = builder
        .body(stream)
        .send()
        .await
        .map_err(|_| Error::IO("Presigned upload request failed".into()))?;
    let status = resp.status();
    match status {
        StatusCode::OK => Ok(()),
        _ => Err(Error::IO(format!(
            "Upload with presigned url failed: {status}"
        ))),
    }
}

pub async fn presign_download_from_stage(
    presigned: PresignedResponse,
    local_path: &Path,
) -> Result<u64> {
    presign_download_with_options(presigned, local_path, &PresignOptions::default()).await
}

pub async fn presign_download_with_options(
    presigned: PresignedResponse,
    local_path: &Path,
    options: &PresignOptions,
) -> Result<u64> {
    let client = options.client(&presigned, "GET").await?;
    if let Some(p) = local_path.parent() {
        tokio::fs::create_dir_all(p).await?;
    }
    let mut builder = client.get(presigned.url);
    for (k, v) in presigned.headers {
        builder = builder.header(k, v);
    }

    let resp = builder
        .send()
        .await
        .map_err(|_| Error::IO("Presigned download request failed".into()))?;
    let status = resp.status();
    match status {
        StatusCode::OK => {
            let mut file = tokio::fs::File::create(local_path).await?;
            let mut body = resp.bytes_stream();
            while let Some(chunk) = body.next().await {
                file.write_all(
                    &chunk.map_err(|_| Error::IO("Presigned download response failed".into()))?,
                )
                .await?;
            }
            file.flush().await?;
            let metadata = file.metadata().await?;
            Ok(metadata.len())
        }
        _ => Err(Error::IO(format!(
            "Download with presigned url failed: {status}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_addresses_are_not_public() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.100.100.200",
            "198.18.0.1",
            "0.1.2.3",
            "240.0.0.1",
            "::1",
            "::ffff:127.0.0.1",
            "fd00::1",
            "fe80::1",
            "2001:db8::1",
            "2002:7f00:1::",
            "3fff::1",
        ] {
            assert!(!public_address(address.parse().unwrap()), "{address}");
        }
        assert!(public_address("8.8.8.8".parse().unwrap()));
        assert!(public_address("2606:4700:4700::1111".parse().unwrap()));
    }

    #[tokio::test]
    async fn unsafe_targets_are_rejected_before_connection() {
        for url in [
            "http://example.com/file",
            "https://127.0.0.1/file",
            "https://169.254.169.254/latest/meta-data",
            "https://[::ffff:127.0.0.1]/file",
            "https://user:secret@example.com/file",
            "file:///etc/passwd",
            "https://example.com/file#fragment",
        ] {
            let response = PresignedResponse {
                method: "PUT".into(),
                headers: BTreeMap::new(),
                url: url.into(),
            };
            let result = PresignOptions::default().client(&response, "PUT").await;
            assert!(result.is_err(), "{url}");
            assert!(!result.unwrap_err().to_string().contains("secret"));
        }
    }

    #[tokio::test]
    async fn redirects_are_not_followed_or_exposed() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut data = [0; 4096];
            assert!(socket.read(&mut data).await.unwrap() > 0);
            socket.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
        });
        let response = PresignedResponse {
            method: "GET".into(),
            headers: BTreeMap::new(),
            url: format!("http://{addr}/file?token=secret"),
        };
        let path = tempfile::tempdir().unwrap();
        let err = presign_download_with_options(
            response,
            &path.path().join("download"),
            &PresignOptions {
                allow_private: true,
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("302"));
        assert!(!err.to_string().contains("secret"));
        assert!(!path.path().join("download").exists());
        task.await.unwrap();
    }
}
