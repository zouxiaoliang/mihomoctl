use std::fmt::Display;

use mihomoctl_core::{
    model::{ConnectionsWithSpeed, Log, Mode, Proxies, Rules, Traffic, Version},
    serde_json::Value,
};
use crossterm::event::{KeyCode as KC, KeyEvent as KE, KeyModifiers as KM};
use log::Level;
use tui::{
    style::{Color, Style},
    text::{Span, Spans},
};

use crate::{
    ui::{api::ApiOperation, components::MovableListItem, utils::AsColor, TuiError, TuiResult},
    Action,
};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Event {
    Quit,
    Action(Action),
    Input(InputEvent),
    Update(UpdateEvent),
    Diagnostic(DiagnosticEvent),
}

impl<'a> MovableListItem<'a> for Event {
    fn to_spans(&self) -> Spans<'a> {
        match self {
            Event::Quit => Spans(vec![]),
            Event::Action(action) => Spans(vec![
                Span::styled("⋉ ", Style::default().fg(Color::Yellow)),
                Span::raw(format!("{:?}", action)),
            ]),
            Event::Update(event) => Spans(vec![
                Span::styled("⇵  ", Style::default().fg(Color::Yellow)),
                Span::raw(event.to_string()),
            ]),
            Event::Input(event) => Spans(vec![
                Span::styled("✜  ", Style::default().fg(Color::Green)),
                Span::raw(format!("{:?}", event)),
            ]),
            Event::Diagnostic(event) => match event {
                DiagnosticEvent::Log(level, payload) => Spans(vec![
                    Span::styled(
                        format!("✇  {:<6}", level),
                        Style::default().fg(level.as_color()),
                    ),
                    Span::raw(payload.to_owned()),
                ]),
            },
        }
    }
}

impl Event {
    pub fn is_quit(&self) -> bool {
        matches!(self, Event::Quit)
    }

    pub fn is_interface(&self) -> bool {
        matches!(self, Event::Input(_))
    }

    pub fn is_update(&self) -> bool {
        matches!(self, Event::Update(_))
    }

    pub fn is_diagnostic(&self) -> bool {
        matches!(self, Event::Diagnostic(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InputEvent {
    Esc,
    TabGoto(u8),
    ToggleDebug,
    ToggleHold,
    List(ListEvent),
    TestLatency,
    TestLatencyAll,
    NextSort,
    PrevSort,
    Other(KE),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEvent {
    pub fast: bool,
    pub code: KC,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UpdateEvent {
    Config(crate::interactive::mihomoctl::model::Config),
    Connection(ConnectionsWithSpeed),
    Version(Version),
    Traffic(Traffic),
    Memory(Value),
    Proxies(Proxies),
    Rules(Rules),
    Log(Log),
    ProxyTestLatencyDone,
    ProxySelectionResult {
        group: String,
        proxy: String,
        error: Option<String>,
    },
    ModeSwitchResult {
        mode: Mode,
        error: Option<String>,
    },
    ConfigReloadResult {
        error: Option<String>,
    },
    ConfigFetchResult {
        error: Option<String>,
    },
    GeoUpdateResult {
        error: Option<String>,
    },
    ConnectionCloseResult {
        all: bool,
        error: Option<String>,
    },
    ServerProbeResult {
        url: url::Url,
        latency_ms: Option<u64>,
        error: Option<String>,
    },
    ApiResult {
        operation: ApiOperation,
        result: String,
    },
}

impl Display for UpdateEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateEvent::Config(x) => write!(f, "{:?}", x),
            UpdateEvent::Connection(x) => write!(f, "{:?}", x),
            UpdateEvent::Version(x) => write!(f, "{:?}", x),
            UpdateEvent::Traffic(x) => write!(f, "{:?}", x),
            UpdateEvent::Memory(x) => write!(f, "{:?}", x),
            UpdateEvent::Proxies(x) => write!(f, "{:?}", x),
            UpdateEvent::Rules(x) => write!(f, "{:?}", x),
            UpdateEvent::Log(x) => write!(f, "{:?}", x),
            UpdateEvent::ProxyTestLatencyDone => write!(f, "Test latency done"),
            UpdateEvent::ProxySelectionResult {
                group,
                proxy,
                error,
            } => match error {
                Some(error) => write!(f, "Failed to switch {group} to {proxy}: {error}"),
                None => write!(f, "Switched {group} to {proxy}"),
            },
            UpdateEvent::ModeSwitchResult { mode, error } => match error {
                Some(error) => write!(f, "Failed to switch mode to {mode:?}: {error}"),
                None => write!(f, "Switched mode to {mode:?}"),
            },
            UpdateEvent::ConfigReloadResult { error } => match error {
                Some(error) => write!(f, "Failed to reload configs: {error}"),
                None => write!(f, "Configs reloaded"),
            },
            UpdateEvent::ConfigFetchResult { error } => match error {
                Some(error) => write!(f, "Failed to fetch configs: {error}"),
                None => write!(f, "Configs fetched"),
            },
            UpdateEvent::GeoUpdateResult { error } => match error {
                Some(error) => write!(f, "Failed to update geo databases: {error}"),
                None => write!(f, "Geo database update started"),
            },
            UpdateEvent::ServerProbeResult {
                url,
                latency_ms,
                error,
            } => match (latency_ms, error) {
                (Some(ms), _) => write!(f, "Server {url} reachable in {ms} ms"),
                (None, Some(error)) => write!(f, "Server {url} unreachable: {error}"),
                (None, None) => write!(f, "Server {url} probe finished"),
            },
            UpdateEvent::ConnectionCloseResult { all, error } => {
                let target = if *all { "all connections" } else { "connection" };
                match error {
                    Some(error) => write!(f, "Failed to close {target}: {error}"),
                    None => write!(f, "Closed {target}"),
                }
            }
            UpdateEvent::ApiResult { operation, result } => {
                write!(f, "{:?}: {}", operation, result)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticEvent {
    Log(Level, String),
}

impl TryFrom<KC> for Event {
    type Error = TuiError;

    fn try_from(value: KC) -> TuiResult<Self> {
        match value {
            KC::Char('q') | KC::Char('x') => Ok(Event::Quit),
            KC::Char('t') => Ok(Event::Input(InputEvent::TestLatency)),
            KC::Esc => Ok(Event::Input(InputEvent::Esc)),
            KC::Char(' ') => Ok(Event::Input(InputEvent::ToggleHold)),
            KC::Char(char) if char.is_ascii_digit() => Ok(Event::Input(InputEvent::TabGoto(
                char.to_digit(10)
                    .expect("char.is_ascii_digit() should be able to parse into number")
                    as u8,
            ))),
            _ => Err(TuiError::TuiInternalErr),
        }
    }
}

impl From<KE> for Event {
    fn from(value: KE) -> Self {
        match (value.modifiers, value.code) {
            (KM::CONTROL, KC::Char('c')) => Self::Quit,
            (KM::CONTROL, KC::Char('d')) => Self::Input(InputEvent::ToggleDebug),
            (_, KC::Esc) => Self::Input(InputEvent::Esc),
            (modi, arrow @ (KC::Left | KC::Right | KC::Up | KC::Down | KC::Enter)) => {
                Event::Input(InputEvent::List(ListEvent {
                    fast: matches!(modi, KM::CONTROL | KM::SHIFT),
                    code: arrow,
                }))
            }
            (KM::ALT, KC::Char('s')) => Self::Input(InputEvent::PrevSort),
            (KM::NONE, _) => Self::Input(InputEvent::Other(value)),
            _ => Self::Input(InputEvent::Other(value)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::{Event, InputEvent};

    #[test]
    fn esc_key_event_maps_to_esc_input() {
        let event = Event::from(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert!(matches!(event, Event::Input(InputEvent::Esc)));
    }
}
