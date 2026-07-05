use std::{
    io::{BufRead, BufReader, Read},
    marker::PhantomData,
    time::Duration,
};

use log::{debug, trace};
use serde::de::DeserializeOwned;
use serde_json::{from_str, json, Value};
use ureq::{Agent, Request};
use url::Url;

use crate::{
    model::{Config, Connections, Delay, Log, Proxies, Proxy, Rules, Traffic, Version},
    Error, Result,
};

trait Convert<T: DeserializeOwned> {
    fn convert(self) -> Result<T>;
}

impl<T: DeserializeOwned> Convert<T> for String {
    fn convert(self) -> Result<T> {
        from_str(&self).map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
pub struct ClashBuilder {
    url: Url,
    secret: Option<String>,
    timeout: Option<Duration>,
}

impl ClashBuilder {
    pub fn new<S: Into<String>>(url: S) -> Result<Self> {
        let mut url_str = url.into();
        // Handle trailling slash
        if !url_str.ends_with('/') {
            url_str += "/";
        };
        let url = Url::parse(&url_str).map_err(|_| Error::url_parse())?;
        Ok(Self {
            url,
            secret: None,
            timeout: None,
        })
    }

    pub fn secret(mut self, secret: Option<String>) -> Self {
        self.secret = secret;
        self
    }

    pub fn timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn build(self) -> Clash {
        let mut clash = Clash::new(self.url);
        clash.secret = self.secret;
        clash.timeout = self.timeout;
        clash
    }
}

/// # Clash API
///
/// Use struct `Clash` for interacting with Mihomo RESTful API.
/// For more information, check <https://github.com/Dreamacro/clash/wiki/external-controller-API-reference###Proxies>,
/// or maybe just read source code of clash
#[derive(Debug, Clone)]
pub struct Clash {
    url: Url,
    secret: Option<String>,
    timeout: Option<Duration>,
    agent: Agent,
}

impl Clash {
    pub fn builder<S: Into<String>>(url: S) -> Result<ClashBuilder> {
        ClashBuilder::new(url)
    }

    pub fn new(url: Url) -> Self {
        debug!("Url of clash RESTful API: {}", url);
        Self {
            url,
            secret: None,
            timeout: None,
            agent: Agent::new(),
        }
    }

    fn build_request(&self, endpoint: &str, method: &str) -> Result<Request> {
        let url = self.url.join(endpoint).map_err(|_| Error::url_parse())?;
        let mut req = self.agent.request_url(method, &url);

        if let Some(timeout) = self.timeout {
            req = req.timeout(timeout)
        }

        if let Some(ref secret) = self.secret {
            req = req.set("Authorization", &format!("Bearer {}", secret))
        }

        Ok(req)
    }

    fn build_request_without_timeout(&self, endpoint: &str, method: &str) -> Result<Request> {
        let url = self.url.join(endpoint).map_err(|_| Error::url_parse())?;
        let mut req = self.agent.request_url(method, &url);

        if let Some(ref secret) = self.secret {
            req = req.set("Authorization", &format!("Bearer {}", secret))
        }

        Ok(req)
    }

    /// Build a WebSocket URL for controller endpoints that support WS.
    pub fn websocket_url(&self, endpoint: &str) -> Result<Url> {
        let mut url = self.url.join(endpoint).map_err(|_| Error::url_parse())?;
        let scheme = match url.scheme() {
            "http" => "ws",
            "https" => "wss",
            "ws" => "ws",
            "wss" => "wss",
            _ => return Err(Error::url_parse()),
        };
        url.set_scheme(scheme).map_err(|_| Error::url_parse())?;

        if let Some(ref secret) = self.secret {
            url.query_pairs_mut().append_pair("token", secret);
        }

        Ok(url)
    }

    /// Connect to a WebSocket endpoint and read one raw message.
    pub fn websocket_next_raw(&self, endpoint: &str) -> Result<String> {
        self.websocket_next_raw_with_timeout(endpoint, Duration::from_secs(30))
    }

    /// Connect to a WebSocket endpoint and read one raw message with read timeout.
    pub fn websocket_next_raw_with_timeout(
        &self,
        endpoint: &str,
        timeout: Duration,
    ) -> Result<String> {
        let url = self.websocket_url(endpoint)?;
        let (mut socket, _) =
            tungstenite::connect(url).map_err(|err| Error::other(err.to_string()))?;
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream
                .set_read_timeout(Some(timeout))
                .map_err(|err| Error::other(err.to_string()))?;
        }

        loop {
            match socket
                .read()
                .map_err(|err| Error::other(err.to_string()))?
            {
                tungstenite::Message::Text(text) => return Ok(text),
                tungstenite::Message::Binary(bytes) => {
                    return String::from_utf8(bytes).map_err(|err| Error::other(err.to_string()));
                }
                tungstenite::Message::Close(_) => {
                    return Err(Error::other("websocket closed".to_owned()));
                }
                _ => {}
            }
        }
    }

    /// Send a oneshot request to the specific endpoint with method, with body
    pub fn oneshot_req_with_body(
        &self,
        endpoint: &str,
        method: &str,
        body: Option<String>,
    ) -> Result<String> {
        trace!("Body: {:#?}", body);
        let resp = if let Some(body) = body {
            self.build_request(endpoint, method)?.send_string(&body)?
        } else {
            self.build_request(endpoint, method)?.call()?
        };

        if resp.status() >= 400 {
            return Err(Error::failed_response(resp.status()));
        }

        let text = resp
            .into_string()
            .map_err(|_| Error::bad_response_encoding())?;
        trace!("Received response: {}", text);

        Ok(text)
    }

    /// Send a oneshot request to the specific endpoint with method, without
    /// body
    pub fn oneshot_req(&self, endpoint: &str, method: &str) -> Result<String> {
        self.oneshot_req_with_body(endpoint, method, None)
    }

    /// Send a longhaul request to the specific endpoint with method,
    /// Underlying is an http stream with chunked-encoding.
    ///
    /// Use [`LongHaul::next_item`], [`LongHaul::next_raw`] or
    /// [`Iterator::next`] to retreive data
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use mihomoctl_core::{ Clash, model::Traffic }; use std::env;
    /// # fn main() {
    /// # let clash = Clash::builder(env::var("PROXY_ADDR").unwrap()).unwrap().build();
    /// let traffics = clash
    ///     .longhaul_req::<Traffic>("traffic", "GET")
    ///     .expect("connect failed");
    ///
    /// // LongHaul implements `Iterator` so you can use iterator combinators
    /// for traffic in traffics.take(3) {
    ///     println!("{:#?}", traffic)
    /// }
    /// # }
    /// ```
    pub fn longhaul_req<T: DeserializeOwned>(
        &self,
        endpoint: &str,
        method: &str,
    ) -> Result<LongHaul<T>> {
        let resp = self
            .build_request_without_timeout(endpoint, method)?
            .call()?;

        if resp.status() >= 400 {
            return Err(Error::failed_response(resp.status()));
        }

        Ok(LongHaul::new(Box::new(resp.into_reader())))
    }

    /// Helper function for method `GET`
    pub fn get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        self.oneshot_req(endpoint, "GET").and_then(Convert::convert)
    }

    /// Helper function for method `DELETE`
    pub fn delete(&self, endpoint: &str) -> Result<()> {
        self.oneshot_req(endpoint, "DELETE").map(|_| ())
    }

    /// Helper function for method `PUT`
    pub fn put<T: DeserializeOwned>(&self, endpoint: &str, body: Option<String>) -> Result<T> {
        self.oneshot_req_with_body(endpoint, "PUT", body)
            .and_then(Convert::convert)
    }

    /// Helper function for method `POST`
    pub fn post<T: DeserializeOwned>(&self, endpoint: &str, body: Option<String>) -> Result<T> {
        self.oneshot_req_with_body(endpoint, "POST", body)
            .and_then(Convert::convert)
    }

    /// Helper function for method `PATCH`
    pub fn patch<T: DeserializeOwned>(&self, endpoint: &str, body: Option<String>) -> Result<T> {
        self.oneshot_req_with_body(endpoint, "PATCH", body)
            .and_then(Convert::convert)
    }

    fn post_empty(&self, endpoint: &str, body: Option<String>) -> Result<()> {
        self.oneshot_req_with_body(endpoint, "POST", body).map(|_| ())
    }

    fn put_empty(&self, endpoint: &str, body: Option<String>) -> Result<()> {
        self.oneshot_req_with_body(endpoint, "PUT", body).map(|_| ())
    }

    fn patch_empty(&self, endpoint: &str, body: Option<String>) -> Result<()> {
        self.oneshot_req_with_body(endpoint, "PATCH", body)
            .map(|_| ())
    }

    fn payload_body(path: Option<&str>, payload: Option<&str>) -> String {
        json!({
            "path": path.unwrap_or_default(),
            "payload": payload.unwrap_or_default(),
        })
        .to_string()
    }

    fn encoded(endpoint: &str) -> String {
        urlencoding::encode(endpoint).into_owned()
    }

    fn delay_endpoint(
        prefix: &str,
        name: &str,
        test_url: &str,
        timeout: u64,
        expected: Option<&str>,
    ) -> String {
        let mut endpoint = format!(
            "{}/{}/delay?url={}&timeout={}",
            prefix,
            Self::encoded(name),
            Self::encoded(test_url),
            timeout
        );
        if let Some(expected) = expected {
            endpoint.push_str("&expected=");
            endpoint.push_str(&Self::encoded(expected));
        }
        endpoint
    }

    fn logs_endpoint(level: Option<&str>, structured: bool) -> String {
        let mut query = Vec::new();
        if let Some(level) = level.filter(|level| !level.is_empty()) {
            query.push(format!("level={}", Self::encoded(level)));
        }
        if structured {
            query.push("format=structured".to_owned());
        }

        if query.is_empty() {
            "logs".to_owned()
        } else {
            format!("logs?{}", query.join("&"))
        }
    }

    fn connections_endpoint(interval: Option<u64>) -> String {
        match interval {
            Some(interval) => format!("connections?interval={interval}"),
            None => "connections".to_owned(),
        }
    }

    /// Get clash version
    pub fn get_version(&self) -> Result<Version> {
        self.get("version")
    }

    /// Get base configs
    pub fn get_configs(&self) -> Result<Config> {
        self.get("configs")
    }

    /// Reloading base configs.
    ///
    /// - `force`: will change ports etc.,
    /// - `path`: the absolute path to config file
    ///
    /// This will **NOT** affect `external-controller` & `secret`
    pub fn reload_configs(&self, force: bool, path: &str) -> Result<()> {
        let body = json!({ "path": path }).to_string();
        debug!("{}", body);
        self.put_empty(
            if force {
                "configs?force=true"
            } else {
                "configs"
            },
            Some(body),
        )
    }

    /// Update base configs.
    pub fn patch_configs(&self, patch: Value) -> Result<Value> {
        self.patch("configs", Some(patch.to_string()))
    }

    /// Update Geo database through `/configs/geo`.
    pub fn update_geo(&self, path: Option<&str>, payload: Option<&str>) -> Result<()> {
        let _ = (path, payload);
        self.post_empty("configs/geo", None)
    }

    /// Restart mihomo.
    pub fn restart(&self, path: Option<&str>, payload: Option<&str>) -> Result<()> {
        self.post_empty("restart", Some(Self::payload_body(path, payload)))
    }

    /// Upgrade mihomo core.
    pub fn upgrade(
        &self,
        channel: Option<&str>,
        force: bool,
        path: Option<&str>,
        payload: Option<&str>,
    ) -> Result<()> {
        let mut params = Vec::new();
        if let Some(channel) = channel {
            params.push(format!("channel={}", Self::encoded(channel)));
        }
        if force {
            params.push("force=true".to_owned());
        }
        let endpoint = if params.is_empty() {
            "upgrade".to_owned()
        } else {
            format!("upgrade?{}", params.join("&"))
        };
        let _ = (path, payload);
        self.post_empty(&endpoint, None)
    }

    /// Upgrade external UI.
    pub fn upgrade_ui(&self) -> Result<()> {
        self.post_empty("upgrade/ui", None)
    }

    /// Upgrade Geo database through `/upgrade/geo`.
    pub fn upgrade_geo(&self, path: Option<&str>, payload: Option<&str>) -> Result<()> {
        let _ = (path, payload);
        self.post_empty("upgrade/geo", None)
    }

    /// Get proxies information
    pub fn get_proxies(&self) -> Result<Proxies> {
        self.get("proxies")
    }

    /// Get rules information
    pub fn get_rules(&self) -> Result<Rules> {
        self.get("rules")
    }

    /// Get specific proxy information
    pub fn get_proxy(&self, proxy: &str) -> Result<Proxy> {
        self.get(&format!("proxies/{}", Self::encoded(proxy)))
    }

    /// Get connections information
    pub fn get_connections(&self) -> Result<Connections> {
        self.get_connections_with_interval(None)
    }

    /// Get connections information with an optional refresh interval.
    pub fn get_connections_with_interval(&self, interval: Option<u64>) -> Result<Connections> {
        self.get(&Self::connections_endpoint(interval))
    }

    /// Close all connections
    pub fn close_connections(&self) -> Result<()> {
        self.delete("connections")
    }

    /// Close specific connection
    pub fn close_one_connection(&self, id: &str) -> Result<()> {
        self.delete(&format!("connections/{}", id))
    }

    /// Get real-time traffic data
    ///
    /// **Note**: This is a longhaul request, which will last forever until
    /// interrupted or disconnected.
    ///
    /// See [`longhaul_req`] for more information
    ///
    /// [`longhaul_req`]: Clash::longhaul_req
    pub fn get_traffic(&self) -> Result<LongHaul<Traffic>> {
        self.longhaul_req("traffic", "GET")
    }

    /// Get real-time logs
    ///
    /// **Note**: This is a longhaul request, which will last forever until
    /// interrupted or disconnected.
    ///
    /// See [`longhaul_req`] for more information
    ///
    /// [`longhaul_req`]: Clash::longhaul_req
    pub fn get_log(&self) -> Result<LongHaul<Log>> {
        self.get_log_with_options(None, false)
    }

    /// Get real-time logs with optional level and structured format.
    pub fn get_log_with_options(
        &self,
        level: Option<&str>,
        structured: bool,
    ) -> Result<LongHaul<Log>> {
        self.longhaul_req(&Self::logs_endpoint(level, structured), "GET")
    }

    /// Get real-time memory data.
    pub fn get_memory(&self) -> Result<LongHaul<Value>> {
        self.longhaul_req("memory", "GET")
    }

    /// Flush fake-ip cache.
    pub fn flush_fakeip_cache(&self) -> Result<()> {
        self.post_empty("cache/fakeip/flush", None)
    }

    /// Flush DNS cache.
    pub fn flush_dns_cache(&self) -> Result<()> {
        self.post_empty("cache/dns/flush", None)
    }

    /// Get proxy group information.
    pub fn get_groups(&self) -> Result<Value> {
        self.get("group")
    }

    /// Get specific proxy group information.
    pub fn get_group(&self, group: &str) -> Result<Value> {
        self.get(&format!("group/{}", Self::encoded(group)))
    }

    /// Get specific proxy group delay test information.
    pub fn get_group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout: u64,
        expected: Option<&str>,
    ) -> Result<Delay> {
        self.get(&Self::delay_endpoint(
            "group", group, test_url, timeout, expected,
        ))
    }

    /// Get specific proxy delay test information
    pub fn get_proxy_delay(&self, proxy: &str, test_url: &str, timeout: u64) -> Result<Delay> {
        self.get_proxy_delay_expected(proxy, test_url, timeout, None)
    }

    /// Get specific proxy delay test information with expected status.
    pub fn get_proxy_delay_expected(
        &self,
        proxy: &str,
        test_url: &str,
        timeout: u64,
        expected: Option<&str>,
    ) -> Result<Delay> {
        self.get(&Self::delay_endpoint(
            "proxies", proxy, test_url, timeout, expected,
        ))
    }

    /// Select specific proxy
    pub fn set_proxygroup_selected(&self, group: &str, proxy: &str) -> Result<()> {
        let body = format!("{{\"name\":\"{}\"}}", proxy);
        self.oneshot_req_with_body(&format!("proxies/{}", Self::encoded(group)), "PUT", Some(body))?;
        Ok(())
    }

    /// Clear fixed selection for a proxy or group.
    pub fn clear_proxy_fixed(&self, proxy: &str) -> Result<()> {
        self.delete(&format!("proxies/{}", Self::encoded(proxy)))
    }

    /// Get proxy providers.
    pub fn get_proxy_providers(&self) -> Result<Value> {
        self.get("providers/proxies")
    }

    /// Get specific proxy provider.
    pub fn get_proxy_provider(&self, provider: &str) -> Result<Value> {
        self.get(&format!("providers/proxies/{}", Self::encoded(provider)))
    }

    /// Update specific proxy provider.
    pub fn update_proxy_provider(&self, provider: &str) -> Result<()> {
        self.put_empty(&format!("providers/proxies/{}", Self::encoded(provider)), None)
    }

    /// Health check specific proxy provider.
    pub fn healthcheck_proxy_provider(&self, provider: &str) -> Result<Value> {
        self.get(&format!(
            "providers/proxies/{}/healthcheck",
            Self::encoded(provider)
        ))
    }

    /// Get proxy in a provider.
    pub fn get_proxy_provider_proxy(&self, provider: &str, proxy: &str) -> Result<Value> {
        self.get(&format!(
            "providers/proxies/{}/{}",
            Self::encoded(provider),
            Self::encoded(proxy)
        ))
    }

    /// Health check a proxy in a provider.
    pub fn healthcheck_proxy_provider_proxy(
        &self,
        provider: &str,
        proxy: &str,
        test_url: &str,
        timeout: u64,
    ) -> Result<Delay> {
        self.get(&format!(
            "providers/proxies/{}/{}/healthcheck?url={}&timeout={}",
            Self::encoded(provider),
            Self::encoded(proxy),
            Self::encoded(test_url),
            timeout
        ))
    }

    /// Disable or enable rules by index.
    pub fn disable_rules<I>(&self, rules: I) -> Result<()>
    where
        I: IntoIterator<Item = (usize, bool)>,
    {
        let body = Value::Object(
            rules
                .into_iter()
                .map(|(index, disabled)| (index.to_string(), Value::Bool(disabled)))
                .collect(),
        )
        .to_string();
        self.patch_empty("rules/disable", Some(body))
    }

    /// Get rule providers.
    pub fn get_rule_providers(&self) -> Result<Value> {
        self.get("providers/rules")
    }

    /// Update specific rule provider.
    pub fn update_rule_provider(&self, provider: &str) -> Result<()> {
        self.put_empty(&format!("providers/rules/{}", Self::encoded(provider)), None)
    }

    /// Query DNS records.
    pub fn dns_query(&self, name: &str, query_type: &str) -> Result<Value> {
        self.get(&format!(
            "dns/query?name={}&type={}",
            Self::encoded(name),
            Self::encoded(query_type)
        ))
    }

    /// Get storage value.
    pub fn get_storage(&self, key: &str) -> Result<Value> {
        self.get(&format!("storage/{}", Self::encoded(key)))
    }

    /// Put storage value.
    pub fn put_storage(&self, key: &str, value: Value) -> Result<()> {
        self.put_empty(&format!("storage/{}", Self::encoded(key)), Some(value.to_string()))
    }

    /// Delete storage value.
    pub fn delete_storage(&self, key: &str) -> Result<()> {
        self.delete(&format!("storage/{}", Self::encoded(key)))
    }

    /// Run active GC.
    pub fn debug_gc(&self) -> Result<()> {
        self.put_empty("debug/gc", None)
    }

    /// Get pprof index.
    pub fn debug_pprof(&self) -> Result<String> {
        self.oneshot_req("debug/pprof", "GET")
    }

    /// Get pprof profile.
    pub fn debug_pprof_profile(&self, profile: &str, raw: bool) -> Result<String> {
        let endpoint = if raw {
            format!("debug/pprof/{}?raw=true", Self::encoded(profile))
        } else {
            format!("debug/pprof/{}", Self::encoded(profile))
        };
        self.oneshot_req(&endpoint, "GET")
    }
}

pub struct LongHaul<T: DeserializeOwned> {
    reader: BufReader<Box<dyn Read + Send>>,
    ty: PhantomData<T>,
}

impl<T: DeserializeOwned> LongHaul<T> {
    pub fn new(reader: Box<dyn Read + Send>) -> Self {
        Self {
            reader: BufReader::new(reader),
            ty: PhantomData,
        }
    }

    pub fn next_item(&mut self) -> Option<Result<T>> {
        Some(self.next_raw()?.and_then(Convert::convert))
    }

    pub fn next_raw(&mut self) -> Option<Result<String>> {
        let mut buf = String::with_capacity(30);
        match self.reader.read_line(&mut buf) {
            Ok(0) => None,
            Ok(_) => Some(Ok(buf)),
            Err(e) => Some(Err(Error::other(format!("{:}", e)))),
        }
    }
}

impl<T: DeserializeOwned> Iterator for LongHaul<T> {
    type Item = Result<T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_item()
    }
}

#[cfg(test)]
mod metacubex_api_tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use serde_json::json;

    use super::Clash;

    #[derive(Debug)]
    struct CapturedRequest {
        method: String,
        path: String,
        body: String,
    }

    fn capture_request<F>(response: &str, call: F) -> CapturedRequest
    where
        F: FnOnce(Clash),
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let response = response.to_owned();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = Vec::new();
            let mut tmp = [0; 1024];

            loop {
                let read = stream.read(&mut tmp).unwrap();
                if read == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..read]);
                if let Some(header_end) = find_headers_end(&buf) {
                    let headers = String::from_utf8_lossy(&buf[..header_end]);
                    let content_len = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if name.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    if buf.len() >= header_end + 4 + content_len {
                        break;
                    }
                }
            }

            let header_end = find_headers_end(&buf).unwrap();
            let head = String::from_utf8_lossy(&buf[..header_end]);
            let request_line = head.lines().next().unwrap();
            let mut request_parts = request_line.split_whitespace();
            let method = request_parts.next().unwrap().to_owned();
            let path = request_parts.next().unwrap().to_owned();
            let body = String::from_utf8_lossy(&buf[header_end + 4..]).to_string();

            let raw_response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(raw_response.as_bytes()).unwrap();

            CapturedRequest { method, path, body }
        });

        let clash = Clash::builder(format!("http://{}", addr)).unwrap().build();
        call(clash);
        handle.join().unwrap()
    }

    fn find_headers_end(buf: &[u8]) -> Option<usize> {
        buf.windows(4).position(|window| window == b"\r\n\r\n")
    }

    fn assert_request<F>(
        response: &str,
        expected_method: &str,
        expected_path: &str,
        expected_body: Option<&str>,
        call: F,
    ) where
        F: FnOnce(Clash),
    {
        let captured = capture_request(response, call);
        assert_eq!(captured.method, expected_method);
        assert_eq!(captured.path, expected_path);
        if let Some(expected_body) = expected_body {
            let expected: serde_json::Value = serde_json::from_str(expected_body).unwrap();
            let actual: serde_json::Value = serde_json::from_str(&captured.body).unwrap();
            assert_eq!(actual, expected);
        } else {
            assert!(captured.body.is_empty());
        }
    }

    #[test]
    fn metacubex_cache_runtime_and_update_wrappers_use_documented_endpoints() {
        assert_request("", "POST", "/cache/fakeip/flush", None, |clash| {
            clash.flush_fakeip_cache().unwrap();
        });
        assert_request("", "POST", "/cache/dns/flush", None, |clash| {
            clash.flush_dns_cache().unwrap();
        });
        assert_request("", "PUT", "/configs?force=true", Some(r#"{"path":"/tmp/a.yaml"}"#), |clash| {
            clash.reload_configs(true, "/tmp/a.yaml").unwrap();
        });
        assert_request(
            r#"{"mixed-port":7890}"#,
            "PATCH",
            "/configs",
            Some(r#"{"mixed-port":7890}"#),
            |clash| {
                clash.patch_configs(json!({ "mixed-port": 7890 })).unwrap();
            },
        );
        assert_request("", "POST", "/configs/geo", None, |clash| {
            clash.update_geo(None, None).unwrap();
        });
        assert_request("", "POST", "/restart", Some(r#"{"path":"","payload":""}"#), |clash| {
            clash.restart(None, None).unwrap();
        });
        assert_request("", "POST", "/upgrade?channel=alpha&force=true", None, |clash| {
            clash.upgrade(Some("alpha"), true, None, None).unwrap();
        });
        assert_request("", "POST", "/upgrade/ui", None, |clash| {
            clash.upgrade_ui().unwrap();
        });
        assert_request("", "POST", "/upgrade/geo", None, |clash| {
            clash.upgrade_geo(None, None).unwrap();
        });
        assert_request("", "PUT", "/debug/gc", None, |clash| {
            clash.debug_gc().unwrap();
        });
    }

    #[test]
    fn metacubex_stream_wrappers_use_documented_optional_params() {
        assert_request("", "GET", "/logs?level=debug&format=structured", None, |clash| {
            clash.get_log_with_options(Some("debug"), true).unwrap();
        });
        assert_request(
            r#"{"connections":[],"downloadTotal":0,"uploadTotal":0}"#,
            "GET",
            "/connections?interval=750",
            None,
            |clash| {
                clash.get_connections_with_interval(Some(750)).unwrap();
            },
        );
    }

    #[test]
    fn metacubex_group_proxy_and_provider_wrappers_use_documented_endpoints() {
        assert_request("{}", "GET", "/group", None, |clash| {
            clash.get_groups().unwrap();
        });
        assert_request("{}", "GET", "/group/Auto", None, |clash| {
            clash.get_group("Auto").unwrap();
        });
        assert_request(
            r#"{"delay":12}"#,
            "GET",
            "/group/Auto/delay?url=http%3A%2F%2Fexample.com&timeout=5000&expected=200-299",
            None,
            |clash| {
                clash
                    .get_group_delay("Auto", "http://example.com", 5000, Some("200-299"))
                    .unwrap();
            },
        );
        assert_request("", "DELETE", "/proxies/Auto", None, |clash| {
            clash.clear_proxy_fixed("Auto").unwrap();
        });
        assert_request(
            r#"{"delay":12}"#,
            "GET",
            "/proxies/Proxy%201/delay?url=http%3A%2F%2Fexample.com&timeout=5000&expected=204",
            None,
            |clash| {
                clash
                    .get_proxy_delay_expected("Proxy 1", "http://example.com", 5000, Some("204"))
                    .unwrap();
            },
        );
        assert_request("{}", "GET", "/providers/proxies", None, |clash| {
            clash.get_proxy_providers().unwrap();
        });
        assert_request("{}", "GET", "/providers/proxies/sub", None, |clash| {
            clash.get_proxy_provider("sub").unwrap();
        });
        assert_request("", "PUT", "/providers/proxies/sub", None, |clash| {
            clash.update_proxy_provider("sub").unwrap();
        });
        assert_request("{}", "GET", "/providers/proxies/sub/healthcheck", None, |clash| {
            clash.healthcheck_proxy_provider("sub").unwrap();
        });
        assert_request("{}", "GET", "/providers/proxies/sub/Proxy%201", None, |clash| {
            clash.get_proxy_provider_proxy("sub", "Proxy 1").unwrap();
        });
        assert_request(
            r#"{"delay":12}"#,
            "GET",
            "/providers/proxies/sub/Proxy%201/healthcheck?url=http%3A%2F%2Fexample.com&timeout=5000",
            None,
            |clash| {
                clash
                    .healthcheck_proxy_provider_proxy("sub", "Proxy 1", "http://example.com", 5000)
                    .unwrap();
            },
        );
    }

    #[test]
    fn metacubex_rules_dns_storage_and_debug_wrappers_use_documented_endpoints() {
        assert_request("", "PATCH", "/rules/disable", Some(r#"{"0":true,"1":false}"#), |clash| {
            clash
                .disable_rules([(0_usize, true), (1, false)])
                .unwrap();
        });
        assert_request("{}", "GET", "/providers/rules", None, |clash| {
            clash.get_rule_providers().unwrap();
        });
        assert_request("", "PUT", "/providers/rules/geosite", None, |clash| {
            clash.update_rule_provider("geosite").unwrap();
        });
        assert_request(
            r#"{"Answer":["1.1.1.1"]}"#,
            "GET",
            "/dns/query?name=example.com&type=A",
            None,
            |clash| {
                clash.dns_query("example.com", "A").unwrap();
            },
        );
        assert_request("null", "GET", "/storage/mihomoctl", None, |clash| {
            clash.get_storage("mihomoctl").unwrap();
        });
        assert_request("", "PUT", "/storage/mihomoctl", Some(r#"{"foo":"bar"}"#), |clash| {
            clash.put_storage("mihomoctl", json!({ "foo": "bar" })).unwrap();
        });
        assert_request("", "DELETE", "/storage/mihomoctl", None, |clash| {
            clash.delete_storage("mihomoctl").unwrap();
        });
        assert_request("pprof", "GET", "/debug/pprof", None, |clash| {
            clash.debug_pprof().unwrap();
        });
        assert_request("heap", "GET", "/debug/pprof/heap?raw=true", None, |clash| {
            clash.debug_pprof_profile("heap", true).unwrap();
        });
    }

    #[test]
    fn websocket_urls_are_built_from_controller_urls() {
        let clash = Clash::builder("http://127.0.0.1:9090/api")
            .unwrap()
            .secret(Some("secret-token".to_owned()))
            .build();
        assert_eq!(
            clash.websocket_url("traffic?interval=1000").unwrap().as_str(),
            "ws://127.0.0.1:9090/api/traffic?interval=1000&token=secret-token"
        );

        let clash = Clash::builder("https://example.com/controller/")
            .unwrap()
            .build();
        assert_eq!(
            clash.websocket_url("connections").unwrap().as_str(),
            "wss://example.com/controller/connections"
        );
    }

    #[test]
    fn websocket_next_raw_reads_one_text_message() {
        use std::net::TcpListener;

        use tungstenite::{accept, Message};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut ws = accept(stream).unwrap();
            ws.write_message(Message::Text(r#"{"up":1,"down":2}"#.to_owned()))
                .unwrap();
        });

        let clash = Clash::builder(format!("http://{}", addr)).unwrap().build();
        assert_eq!(
            clash.websocket_next_raw("traffic").unwrap(),
            r#"{"up":1,"down":2}"#
        );
        handle.join().unwrap();
    }

    #[test]
    fn websocket_next_raw_times_out_when_no_message_arrives() {
        use std::{net::TcpListener, time::Duration};

        use tungstenite::accept;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let _ws = accept(stream).unwrap();
            thread::sleep(Duration::from_millis(150));
        });

        let clash = Clash::builder(format!("http://{}", addr)).unwrap().build();
        assert!(clash
            .websocket_next_raw_with_timeout("traffic", Duration::from_millis(20))
            .is_err());
        handle.join().unwrap();
    }
}
