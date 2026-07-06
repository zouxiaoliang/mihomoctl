use std::{
    fmt::Display,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    ops::{Deref, DerefMut},
    path::Path,
    time::Duration,
};

use mihomoctl_core::serde_json::{from_str as json_from_str, Value};
use mihomoctl_core::{Clash, ClashBuilder};
use log::{debug, info};
use ron::{from_str, ser::PrettyConfig};
use serde::{Deserialize, Serialize};
use url::Url;

use super::{ConfigData, InteractiveError, InteractiveResult};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ControllerKind {
    Clash,
    Mihomo,
}

impl ControllerKind {
    pub fn detect_from_version_body(body: &str) -> Option<Self> {
        let value: Value = json_from_str(body).ok()?;
        let object = value.as_object()?;

        let body = body.to_ascii_lowercase();
        if body.contains("mihomo")
            || object
                .get("meta")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            return Some(Self::Mihomo);
        }

        object.get("version").map(|_| Self::Clash)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clash => "clash",
            Self::Mihomo => "mihomo",
        }
    }
}

impl Default for ControllerKind {
    fn default() -> Self {
        Self::Mihomo
    }
}

impl Display for ControllerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn detect_controller_kind(
    url: &Url,
    secret: Option<&str>,
    timeout: Option<Duration>,
) -> Option<ControllerKind> {
    let clash = ClashBuilder::new(url.clone())
        .ok()?
        .secret(secret.map(ToOwned::to_owned))
        .timeout(timeout)
        .build();
    let body = clash.oneshot_req("version", "GET").ok()?;
    ControllerKind::detect_from_version_body(&body)
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Server {
    pub url: url::Url,
    pub secret: Option<String>,
    #[serde(default)]
    pub kind: ControllerKind,
}

impl Server {
    pub fn into_clash_with_timeout(self, timeout: Option<Duration>) -> InteractiveResult<Clash> {
        Ok(self.into_clash_builder()?.timeout(timeout).build())
    }

    pub fn into_clash(self) -> InteractiveResult<Clash> {
        self.into_clash_with_timeout(None)
    }

    pub fn into_clash_builder(self) -> InteractiveResult<ClashBuilder> {
        Ok(ClashBuilder::new(self.url)?.secret(self.secret))
    }
}

impl Display for Server {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Server ({}, {})", self.url, self.kind)
    }
}

impl TryInto<Clash> for Server {
    type Error = InteractiveError;

    fn try_into(self) -> std::result::Result<Clash, Self::Error> {
        self.into_clash()
    }
}

impl TryInto<ClashBuilder> for Server {
    type Error = InteractiveError;

    fn try_into(self) -> std::result::Result<ClashBuilder, Self::Error> {
        self.into_clash_builder()
    }
}

#[derive(Debug)]
pub struct Config {
    inner: ConfigData,
    file: File,
}

// TODO: use config crate
impl Config {
    pub fn from_dir<P: AsRef<Path>>(path: P) -> InteractiveResult<Self> {
        let path = path.as_ref();

        debug!("Open config file @ {}", path.display());

        let mut this = if !path.exists() {
            info!("Config file not exist, creating new one");
            Self {
                inner: ConfigData::default(),
                file: File::create(path).map_err(InteractiveError::ConfigFileIoError)?,
            }
        } else {
            debug!("Reading and parsing config file");

            let mut file = OpenOptions::new()
                .read(true)
                .open(path)
                .map_err(InteractiveError::ConfigFileIoError)?;

            let mut buf = match file.metadata() {
                Ok(meta) => String::with_capacity(meta.len() as usize),
                Err(_) => String::new(),
            };

            file.read_to_string(&mut buf)
                .map_err(InteractiveError::ConfigFileIoError)?;

            debug!("Raw config:\n{}", buf);

            let inner = from_str(&buf)?;

            drop(file);

            debug!("Content read");

            let file = File::create(path).map_err(InteractiveError::ConfigFileIoError)?;

            Self { inner, file }
        };

        this.write()?;
        Ok(this)
    }

    pub fn write(&mut self) -> InteractiveResult<()> {
        let pretty_config = PrettyConfig::default().indentor("  ".to_owned());

        // Reset the file - Move cursor to 0 and truncate to 0
        self.file
            .seek(SeekFrom::Start(0))
            .and_then(|_| self.file.set_len(0))
            .map_err(InteractiveError::ConfigFileIoError)?;

        ron::ser::to_writer_pretty(&mut self.file, &self.inner, pretty_config)?;
        self.file
            .flush()
            .map_err(InteractiveError::ConfigFileIoError)?;

        Ok(())
    }

    pub fn using_server(&self) -> Option<&Server> {
        match self.using {
            Some(ref using) => self.servers.iter().find(|x| &x.url == using),
            None => None,
        }
    }

    pub fn use_server(&mut self, url: Url) -> InteractiveResult<()> {
        match self.get_server(&url) {
            Some(_s) => {
                self.using = Some(url);
                Ok(())
            }
            None => Err(InteractiveError::ServerNotFound),
        }
    }

    pub fn get_server(&mut self, url: &Url) -> Option<&Server> {
        self.servers.iter().find(|x| &x.url == url)
    }

    pub fn get_inner(&self) -> &ConfigData {
        &self.inner
    }
}

impl Deref for Config {
    type Target = ConfigData;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Config {
    fn deref_mut(&mut self) -> &mut ConfigData {
        &mut self.inner
    }
}

#[test]
fn test_config() {
    let _ = pretty_env_logger::formatted_builder()
        .filter_level(log::LevelFilter::Debug)
        .try_init();

    let mut config = Config::from_dir("/tmp/test.ron").unwrap();
    config.write().unwrap();
    config.servers.push(Server {
        url: url::Url::parse("http://127.0.0.1:9090").unwrap(),
        secret: None,
        kind: ControllerKind::Mihomo,
    });
    config.write().unwrap();
}

#[test]
fn old_server_config_defaults_to_mihomo_kind() {
    let server: Server = ron::from_str(
        r#"(
            url: "http://127.0.0.1:9090/",
            secret: None,
        )"#,
    )
    .unwrap();

    assert_eq!(server.kind, ControllerKind::Mihomo);
}

#[test]
fn new_server_config_serializes_kind() {
    let server = Server {
        url: url::Url::parse("http://127.0.0.1:9090/").unwrap(),
        secret: Some("token".to_owned()),
        kind: ControllerKind::Clash,
    };

    let serialized = ron::to_string(&server).unwrap();
    assert!(serialized.contains("kind"));
    assert!(serialized.contains("clash"));
}

#[test]
fn detect_controller_kind_from_version_payloads() {
    assert_eq!(
        ControllerKind::detect_from_version_body(r#"{"version":"Mihomo v1.18.0"}"#),
        Some(ControllerKind::Mihomo)
    );
    assert_eq!(
        ControllerKind::detect_from_version_body(r#"{"version":"1.18.0","meta":true}"#),
        Some(ControllerKind::Mihomo)
    );
    assert_eq!(
        ControllerKind::detect_from_version_body(r#"{"version":"1.18.0"}"#),
        Some(ControllerKind::Clash)
    );
    assert_eq!(ControllerKind::detect_from_version_body("not json"), None);
}

#[test]
fn detect_controller_kind_returns_none_when_probe_fails() {
    let url = Url::parse("ftp://127.0.0.1:9090").unwrap();
    assert_eq!(
        detect_controller_kind(&url, None, Some(Duration::from_millis(20))),
        None
    );
}
