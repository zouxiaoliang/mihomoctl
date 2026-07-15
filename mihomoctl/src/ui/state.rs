use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use mihomoctl_core::{
    model::{ConnectionWithSpeed, Log, Mode, Rule, Traffic, Version},
    serde_json::Value,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use smart_default::SmartDefault;
use tui::{
    style::{Color, Style},
    text::{Span, Spans},
};

use url::Url;

use crate::{
    interactive::{ConSort, ControllerKind, Noop, RuleSort, Server},
    ui::{
        api::{self, ApiListState},
        components::{MovableListManage, MovableListManager, MovableListState, ProxyTree},
        get_config, TuiResult,
    },
    Action, ConfigState, Event, InputEvent, UpdateEvent,
};

pub(crate) type LogListState<'a> = MovableListState<'a, Log, Noop>;
pub(crate) type ConListState<'a> = MovableListState<'a, ConnectionWithSpeed, ConSort>;
pub(crate) type RuleListState<'a> = MovableListState<'a, Rule, RuleSort>;
pub(crate) type DebugListState<'a> = MovableListState<'a, Event, Noop>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeSwitchPopup {
    pub current: Mode,
    pub index: usize,
}

impl ModeSwitchPopup {
    pub const MODES: [Mode; 3] = [Mode::Rule, Mode::Global, Mode::Direct];

    pub fn new(current: Mode) -> Self {
        let index = Self::MODES
            .iter()
            .position(|mode| *mode == current)
            .unwrap_or(0);
        Self { current, index }
    }

    pub fn selected(&self) -> Mode {
        Self::MODES[self.index]
    }
}

/// A generic "press Enter to confirm" popup carrying the action to run
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmPopup {
    pub title: String,
    pub body: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResultPopup {
    pub title: String,
    pub body: String,
    pub offset: crate::Coord,
}

/// Transient confirmation shown after e.g. a successful server switch.
/// Dismissed by any key, or automatically after [`NoticePopup::TTL`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoticePopup {
    pub title: String,
    pub body: String,
    pub created: Instant,
}

impl NoticePopup {
    pub const TTL: Duration = Duration::from_secs(5);

    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            created: Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed() >= Self::TTL
    }

    pub fn remaining_secs(&self) -> u64 {
        Self::TTL
            .saturating_sub(self.created.elapsed())
            .as_secs_f64()
            .ceil() as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSwitchPopup {
    pub servers: Vec<Server>,
    pub active: Option<Url>,
    pub index: usize,
    pub mode: ServerPopupMode,
    /// Transient feedback shown at the bottom of the popup
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ServerPopupMode {
    #[default]
    List,
    ConfirmDelete,
    Add(ServerForm),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServerForm {
    pub url: String,
    pub secret: String,
    pub kind: ControllerKind,
    pub field: ServerFormField,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ServerFormField {
    #[default]
    Url,
    Secret,
    Kind,
}

impl ServerFormField {
    fn next(self) -> Self {
        match self {
            Self::Url => Self::Secret,
            Self::Secret => Self::Kind,
            Self::Kind => Self::Url,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::Url => Self::Kind,
            Self::Secret => Self::Url,
            Self::Kind => Self::Secret,
        }
    }
}

#[derive(Debug, Clone, SmartDefault)]
pub struct TuiStates<'a> {
    pub should_quit: bool,
    #[default(_code = "Instant::now()")]
    pub start_time: Instant,
    pub version: Option<Version>,
    pub traffics: Vec<Traffic>,
    pub memory: Option<Value>,
    pub max_traffic: Traffic,
    pub all_events_recv: usize,
    pub page_index: u8,
    pub show_debug: bool,
    pub proxy_tree: ProxyTree<'a>,
    pub rule_freq: HashMap<String, usize>,
    // (upload_size, download_size)
    pub con_size: (u64, u64),

    #[default(_code = "{
        let mut ret = MovableListState::default();
        ret.with_index().dsc_index().pausable();
        ret
    }")]
    pub log_state: LogListState<'a>,
    #[default(_code = "default_connection_state()")]
    pub con_state: ConListState<'a>,
    pub rule_state: RuleListState<'a>,
    pub debug_state: DebugListState<'a>,
    #[default(_code = "api::config_core_api_state()")]
    pub config_core_api_state: ApiListState<'a>,
    #[default(_code = "api::dns_api_state()")]
    pub dns_api_state: ApiListState<'a>,
    #[default(_code = "api::default_api_state()")]
    pub api_state: ApiListState<'a>,
    pub api_result_popup: Option<ApiResultPopup>,
    pub notice_popup: Option<NoticePopup>,
    pub server_popup: Option<ServerSwitchPopup>,
    pub mode_popup: Option<ModeSwitchPopup>,
    pub confirm_popup: Option<ConfirmPopup>,
    /// Scroll offset of the help popup; `Some` while help is shown
    pub help_popup: Option<crate::Coord>,
    pub switch_to_server: Option<Url>,
    pub config_state: ConfigState,
}

fn default_connection_state<'a>() -> ConListState<'a> {
    let mut state = MovableListState::default();
    state.pausable();
    state.header(Spans::from(Span::styled(
        format!(
            "{:45} {:16} {:16} {:16} {:16} {:13} {:18} {:44} {}",
            "HOST", "DOWN", "DOWN/S", "UP", "UP/S", "TIME", "RULE", "SOURCE -> DEST", "CHAIN"
        ),
        Style::default().fg(Color::DarkGray),
    )));
    state.placeholder("No active connections");
    state
}

impl<'a> TuiStates<'a> {
    pub const TITLES: &'static [&'static str] = &[
        "Status",
        "Proxies",
        "Rules",
        "Conns",
        "Logs",
        "Core",
        "DNS",
        "APIs",
        "Debug",
    ];

    pub fn for_controller_kind(kind: ControllerKind) -> Self {
        Self {
            config_core_api_state: api::config_core_api_state_for_kind(kind),
            dns_api_state: api::dns_api_state_for_kind(kind),
            api_state: api::api_state_for_kind(kind),
            ..Self::default()
        }
    }

    pub fn handle(&mut self, event: Event) -> TuiResult<Option<Action>> {
        // Any event (updates included) is an occasion to expire the notice,
        // so it fades out on its own without user interaction.
        if self
            .notice_popup
            .as_ref()
            .is_some_and(NoticePopup::is_expired)
        {
            self.notice_popup = None;
        }
        self.all_events_recv += 1;
        if self.debug_state.len() >= 300 {
            self.drop_events(100);
        }
        self.debug_state.push(event.to_owned());

        match event {
            Event::Quit => {
                self.should_quit = true;
                Ok(None)
            }
            Event::Input(event) => {
                // A transient notice is dismissed by any key press
                if self.notice_popup.take().is_some() {
                    return Ok(None);
                }
                self.handle_input(event)
            }
            Event::Update(update) => self.handle_update(update),
            _ => Ok(None),
        }
    }

    #[inline]
    pub fn page_len(&mut self) -> usize {
        if self.show_debug {
            Self::TITLES.len()
        } else {
            Self::TITLES.len() - 1
        }
    }

    #[inline]
    pub fn title(&self) -> &str {
        Self::TITLES[self.page_index as usize]
    }

    fn active_list<'own>(&'own mut self) -> Option<MovableListManager<'a, 'own>> {
        match self.title() {
            "Rules" => Some(MovableListManager::Rule(&mut self.rule_state)),
            "Debug" => Some(MovableListManager::Event(&mut self.debug_state)),
            "Logs" => Some(MovableListManager::Log(&mut self.log_state)),
            "Conns" => Some(MovableListManager::Connection(&mut self.con_state)),
            "Proxies" => Some(MovableListManager::Proxy(&mut self.proxy_tree)),
            "Core" => Some(MovableListManager::Api(&mut self.config_core_api_state)),
            "DNS" => Some(MovableListManager::Api(&mut self.dns_api_state)),
            "APIs" => Some(MovableListManager::Api(&mut self.api_state)),
            _ => None,
        }
    }

    pub fn active_api_state(&self) -> Option<&ApiListState<'a>> {
        match self.title() {
            "Core" => Some(&self.config_core_api_state),
            "DNS" => Some(&self.dns_api_state),
            "APIs" => Some(&self.api_state),
            _ => None,
        }
    }

    fn active_api_state_mut(&mut self) -> Option<&mut ApiListState<'a>> {
        match self.page_index as usize {
            5 => Some(&mut self.config_core_api_state),
            6 => Some(&mut self.dns_api_state),
            7 => Some(&mut self.api_state),
            _ => None,
        }
    }

    fn is_api_page(&self) -> bool {
        self.active_api_state().is_some()
    }

    fn begin_list_search(&mut self) -> bool {
        match self.title() {
            "Rules" => {
                self.rule_state.begin_search();
            }
            "Conns" => {
                self.con_state.begin_search();
            }
            "Logs" => {
                self.log_state.begin_search();
            }
            _ => return false,
        };
        true
    }

    fn cancel_list_search_if_active(&mut self) -> bool {
        match self.title() {
            "Rules" if self.rule_state.is_searching() => {
                self.rule_state.cancel_search();
            }
            "Conns" if self.con_state.is_searching() => {
                self.con_state.cancel_search();
            }
            "Logs" if self.log_state.is_searching() => {
                self.log_state.cancel_search();
            }
            _ => return false,
        };
        true
    }

    fn handle_list_search_key(&mut self, event: &KeyEvent) -> bool {
        match event.code {
            KeyCode::Char(ch) => match self.title() {
                "Rules" if self.rule_state.is_searching() => {
                    self.rule_state.input_search_char(ch);
                }
                "Conns" if self.con_state.is_searching() => {
                    self.con_state.input_search_char(ch);
                }
                "Logs" if self.log_state.is_searching() => {
                    self.log_state.input_search_char(ch);
                }
                _ => return false,
            },
            KeyCode::Backspace => match self.title() {
                "Rules" if self.rule_state.is_searching() => {
                    self.rule_state.backspace_search();
                }
                "Conns" if self.con_state.is_searching() => {
                    self.con_state.backspace_search();
                }
                "Logs" if self.log_state.is_searching() => {
                    self.log_state.backspace_search();
                }
                _ => return false,
            },
            KeyCode::Enter => {
                return self.cancel_list_search_if_active();
            }
            _ => return false,
        };
        true
    }

    fn handle_update(&mut self, update: UpdateEvent) -> TuiResult<Option<Action>> {
        match update {
            UpdateEvent::Config(config) => self.config_state.update_clash(config),
            UpdateEvent::Connection(connection) => {
                if !self.con_state.is_paused() {
                    self.con_size = (connection.upload_total, connection.download_total);
                    self.con_state.sorted_merge(connection.connections);
                    self.con_state.with_index();
                }
            }
            UpdateEvent::Version(version) => self.version = Some(version),
            UpdateEvent::Traffic(traffic) => {
                let Traffic { up, down } = traffic;
                self.max_traffic.up = self.max_traffic.up.max(up);
                self.max_traffic.down = self.max_traffic.down.max(down);
                self.traffics.push(traffic)
            }
            UpdateEvent::Memory(memory) => self.memory = Some(memory),
            UpdateEvent::Proxies(proxies) => {
                let mut new_tree = Into::<ProxyTree>::into(proxies);
                new_tree.sort_groups_with_frequency(&self.rule_freq);
                self.proxy_tree.replace_with(new_tree);
            }
            UpdateEvent::Log(log) => {
                if !self.log_state.is_paused() {
                    self.log_state.push(log);
                }
            }
            UpdateEvent::Rules(rules) => {
                self.rule_freq = rules.owned_frequency();
                self.rule_state.sorted_merge(rules.rules);
            }
            UpdateEvent::ProxyTestLatencyDone => {
                self.proxy_tree.end_testing();
            }
            UpdateEvent::ProxySelectionResult {
                group,
                proxy,
                error,
            } => match error {
                Some(error) => {
                    self.api_result_popup = Some(ApiResultPopup {
                        title: "Switch Node Failed".to_owned(),
                        body: format!("{group} → {proxy}\n\n{error}"),
                        offset: Default::default(),
                    });
                }
                None => {
                    self.notice_popup = Some(NoticePopup::new(
                        "Node Switched",
                        format!("{group} → {proxy}"),
                    ));
                }
            },
            UpdateEvent::ModeSwitchResult { mode, error } => match error {
                Some(error) => {
                    self.api_result_popup = Some(ApiResultPopup {
                        title: "Switch Mode Failed".to_owned(),
                        body: format!("mode → {mode:?}\n\n{error}"),
                        offset: Default::default(),
                    });
                }
                None => {
                    self.notice_popup =
                        Some(NoticePopup::new("Mode Switched", format!("mode → {mode:?}")));
                }
            },
            UpdateEvent::ConfigFetchResult { error } => match error {
                Some(error) => {
                    self.api_result_popup = Some(ApiResultPopup {
                        title: "Fetch Configs Failed".to_owned(),
                        body: error,
                        offset: Default::default(),
                    });
                }
                None => {
                    self.notice_popup = Some(NoticePopup::new(
                        "Configs Refreshed",
                        "Fetched the latest configs from the server".to_owned(),
                    ));
                }
            },
            UpdateEvent::ConnectionCloseResult { all, error } => {
                let target = if all { "All Connections" } else { "Connection" };
                match error {
                    Some(error) => {
                        self.api_result_popup = Some(ApiResultPopup {
                            title: format!("Close {target} Failed"),
                            body: error,
                            offset: Default::default(),
                        });
                    }
                    None => {
                        self.notice_popup = Some(NoticePopup::new(
                            format!("{target} Closed"),
                            "The server has dropped the connection(s)".to_owned(),
                        ));
                    }
                }
            }
            UpdateEvent::GeoUpdateResult { error } => match error {
                Some(error) => {
                    self.api_result_popup = Some(ApiResultPopup {
                        title: "Update Geo Failed".to_owned(),
                        body: error,
                        offset: Default::default(),
                    });
                }
                None => {
                    self.notice_popup = Some(NoticePopup::new(
                        "Geo Update Started",
                        "The server is downloading the geo databases".to_owned(),
                    ));
                }
            },
            UpdateEvent::ConfigReloadResult { error } => match error {
                Some(error) => {
                    self.api_result_popup = Some(ApiResultPopup {
                        title: "Reload Configs Failed".to_owned(),
                        body: error,
                        offset: Default::default(),
                    });
                }
                None => {
                    self.notice_popup = Some(NoticePopup::new(
                        "Configs Reloaded",
                        "Reloaded from the config file on the server".to_owned(),
                    ));
                }
            },
            UpdateEvent::ApiResult { operation, result } => {
                let popup_body = result.clone();
                api::update_result(&mut self.config_core_api_state, operation, result.clone());
                api::update_result(&mut self.dns_api_state, operation, result.clone());
                api::update_result(&mut self.api_state, operation, result);
                self.api_result_popup = Some(ApiResultPopup {
                    title: format!("{} {}", operation.method(), operation.title()),
                    body: popup_body,
                    offset: Default::default(),
                });
            }
        }
        Ok(None)
    }

    fn handle_input(&mut self, event: InputEvent) -> TuiResult<Option<Action>> {
        match event {
            InputEvent::TabGoto(index) => {
                if index >= 1 && index <= self.page_len() as u8 {
                    self.page_index = index - 1
                }
            }
            InputEvent::ToggleDebug => {
                self.show_debug = !self.show_debug;
                // On the debug page
                if self.page_index == Self::TITLES.len() as u8 - 1 {
                    self.page_index -= 1;
                } else if self.show_debug {
                    self.page_index = self.debug_page_index()
                }
            }
            InputEvent::Esc => {
                if self.help_popup.take().is_some() {
                    return Ok(None);
                }
                if let Some(popup) = self.server_popup.as_mut() {
                    // Esc backs out of a sub-mode first, then closes the popup
                    if matches!(popup.mode, ServerPopupMode::List) {
                        self.server_popup = None;
                    } else {
                        popup.mode = ServerPopupMode::List;
                        popup.message = None;
                    }
                    return Ok(None);
                }
                if self.mode_popup.take().is_some() {
                    return Ok(None);
                }
                if self.confirm_popup.take().is_some() {
                    return Ok(None);
                }
                if self.api_result_popup.take().is_some() {
                    return Ok(None);
                }
                if let Some(api_state) = self.active_api_state_mut() {
                    if api::is_editing(api_state) {
                        api::end_edit(api_state);
                        return Ok(None);
                    }
                }
                if self.title() == "Proxies" && self.proxy_tree.is_searching() {
                    self.proxy_tree.cancel_search();
                    return Ok(None);
                }
                if self.cancel_list_search_if_active() {
                    return Ok(None);
                }
                if let Some(api_state) = self.active_api_state_mut() {
                    api::cancel_confirmation(api_state);
                }
                if let Some(mut list) = self.active_list() {
                    list.end();
                }
            }
            InputEvent::ToggleHold => {
                if let Some(mut list) = self.active_list() {
                    list.toggle();
                }
            }
            InputEvent::List(list_event) => {
                if self.help_popup.is_some() {
                    self.handle_help_popup_scroll(list_event);
                    return Ok(None);
                }
                if self.server_popup.is_some() {
                    self.handle_server_popup_key(list_event);
                    return Ok(None);
                }
                if self.mode_popup.is_some() {
                    return Ok(self.handle_mode_popup_key(list_event));
                }
                if self.confirm_popup.is_some() {
                    if list_event.code == KeyCode::Enter {
                        return Ok(self.confirm_popup.take().map(|popup| popup.action));
                    }
                    return Ok(None);
                }
                if self.api_result_popup.is_some() {
                    self.handle_api_result_popup_scroll(list_event);
                    return Ok(None);
                }
                if let Some(api_state) = self.active_api_state_mut() {
                    if api::is_editing(api_state) {
                        if list_event.code == KeyCode::Enter {
                            api::end_edit(api_state);
                            return Ok(api::submit_current(api_state));
                        }
                        return Ok(None);
                    }
                }
                if list_event.code == KeyCode::Enter {
                    if self.title() == "Proxies" && self.proxy_tree.is_searching() {
                        self.proxy_tree.cancel_search();
                        return Ok(None);
                    }
                    if self.cancel_list_search_if_active() {
                        return Ok(None);
                    }
                }
                if self.is_api_page() && list_event.code == KeyCode::Enter {
                    let Some(api_state) = self.active_api_state_mut() else {
                        return Ok(None);
                    };
                    api_state.hold();
                    if api::current_needs_input(api_state) {
                        api::begin_edit(api_state);
                        return Ok(None);
                    }
                    return Ok(api::submit_current(api_state));
                }
                if let Some(mut list) = self.active_list() {
                    return Ok(list.handle(list_event));
                }
            }
            InputEvent::TestLatency => {
                if self.title() == "Proxies" && !self.proxy_tree.is_testing() {
                    self.proxy_tree.start_testing();
                    let group = self.proxy_tree.current_group();
                    let proxies = group
                        .members()
                        .iter()
                        .filter(|x| x.proxy_type().is_normal())
                        .map(|x| x.name().into())
                        .collect();
                    return Ok(Some(Action::TestLatency { proxies }));
                }
            }
            InputEvent::TestLatencyAll => {
                if self.title() == "Proxies" && !self.proxy_tree.is_testing() {
                    let proxies = self.proxy_tree.unique_normal_members();
                    if proxies.is_empty() {
                        return Ok(None);
                    }
                    self.proxy_tree.start_testing();
                    return Ok(Some(Action::TestLatency { proxies }));
                }
            }
            InputEvent::NextSort => {
                if let Some(mut list) = self.active_list() {
                    list.next_sort();
                }
            }
            InputEvent::PrevSort => {
                if let Some(mut list) = self.active_list() {
                    list.prev_sort();
                }
            }
            InputEvent::Other(event) => return self.handle_key_event(event),
        }
        Ok(None)
    }

    fn handle_help_popup_scroll(&mut self, event: crate::ListEvent) {
        let Some(offset) = self.help_popup.as_mut() else {
            return;
        };
        let step = if event.fast { 8 } else { 1 };
        match event.code {
            KeyCode::Up => offset.y = offset.y.saturating_sub(step),
            KeyCode::Down => offset.y = offset.y.saturating_add(step),
            KeyCode::Left => offset.x = offset.x.saturating_sub(step * 4),
            KeyCode::Right => offset.x = offset.x.saturating_add(step * 4),
            _ => {}
        }
    }

    fn handle_mode_popup_key(&mut self, event: crate::ListEvent) -> Option<Action> {
        let popup = self.mode_popup.as_mut()?;
        match event.code {
            KeyCode::Up => {
                popup.index = popup
                    .index
                    .checked_sub(1)
                    .unwrap_or(ModeSwitchPopup::MODES.len() - 1);
            }
            KeyCode::Down => {
                popup.index = (popup.index + 1) % ModeSwitchPopup::MODES.len();
            }
            KeyCode::Enter => {
                let mode = popup.selected();
                let current = popup.current;
                self.mode_popup = None;
                if mode != current {
                    return Some(Action::SetMode { mode });
                }
            }
            _ => {}
        }
        None
    }

    fn handle_api_result_popup_scroll(&mut self, event: crate::ListEvent) {
        let Some(popup) = self.api_result_popup.as_mut() else {
            return;
        };
        let step = if event.fast { 8 } else { 1 };
        match event.code {
            KeyCode::Up => popup.offset.y = popup.offset.y.saturating_sub(step),
            KeyCode::Down => popup.offset.y = popup.offset.y.saturating_add(step),
            KeyCode::Left => popup.offset.x = popup.offset.x.saturating_sub(step * 4),
            KeyCode::Right => popup.offset.x = popup.offset.x.saturating_add(step * 4),
            _ => {}
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent) -> TuiResult<Option<Action>> {
        if self.help_popup.is_some() {
            // Shift-H toggles the help popup closed again
            if event.code == KeyCode::Char('H') {
                self.help_popup = None;
            }
            return Ok(None);
        }
        if self.server_popup.is_some() {
            self.handle_server_popup_char(event);
            return Ok(None);
        }
        if self.mode_popup.is_some() {
            // Only arrows / Enter / Esc drive the mode popup
            return Ok(None);
        }
        if self.confirm_popup.is_some() {
            // Only Enter / Esc drive the confirm popup
            return Ok(None);
        }
        if self.api_result_popup.is_some() {
            return Ok(None);
        }

        if self.title() == "Proxies" {
            if self.proxy_tree.is_searching() {
                match event.code {
                    KeyCode::Char(ch) => {
                        self.proxy_tree.input_search_char(ch);
                        return Ok(None);
                    }
                    KeyCode::Backspace => {
                        self.proxy_tree.backspace_search();
                        return Ok(None);
                    }
                    KeyCode::Enter => {
                        self.proxy_tree.cancel_search();
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            if event.modifiers == KeyModifiers::NONE && event.code == KeyCode::Char('/') {
                self.proxy_tree.begin_search();
                return Ok(None);
            }
        }

        if self.handle_list_search_key(&event) {
            return Ok(None);
        }

        if event.modifiers == KeyModifiers::NONE
            && event.code == KeyCode::Char('/')
            && self.begin_list_search()
        {
            return Ok(None);
        }

        if self.is_api_page() {
            let Some(api_state) = self.active_api_state_mut() else {
                return Ok(None);
            };
            if api::is_editing(api_state) {
                match (event.modifiers, event.code) {
                    (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                        api::clear_current_param(api_state);
                        return Ok(None);
                    }
                    (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                        api::input_char(api_state, ch);
                        return Ok(None);
                    }
                    (_, KeyCode::Backspace) => {
                        api::backspace_current_param(api_state);
                        return Ok(None);
                    }
                    (_, KeyCode::Tab) => {
                        api::next_param(api_state);
                        return Ok(None);
                    }
                    _ => {}
                }
            }

            match event.code {
                KeyCode::Char('i') => {
                    api::begin_edit(api_state);
                    return Ok(None);
                }
                KeyCode::Char('p') => {
                    self.pick_api_param();
                    return Ok(None);
                }
                KeyCode::Tab => {
                    api::next_param(api_state);
                    return Ok(None);
                }
                _ => {}
            }
        }

        match (event.modifiers, event.code) {
            (KeyModifiers::NONE, KeyCode::Char('q' | 'x')) => {
                self.should_quit = true;
            }
            (_, KeyCode::Char('S')) => {
                self.open_server_popup();
            }
            (_, KeyCode::Char('H')) => {
                self.help_popup = Some(Default::default());
            }
            (KeyModifiers::NONE, KeyCode::Char('t')) => {
                return self.handle_input(InputEvent::TestLatency);
            }
            (_, KeyCode::Char('T')) => {
                return self.handle_input(InputEvent::TestLatencyAll);
            }
            (KeyModifiers::NONE, KeyCode::Char(' ')) => {
                return self.handle_input(InputEvent::ToggleHold);
            }
            (KeyModifiers::NONE, KeyCode::Char(ch)) if ch.is_ascii_digit() => {
                return self.handle_input(InputEvent::TabGoto(
                    ch.to_digit(10)
                        .expect("char.is_ascii_digit() should parse")
                        as u8,
                ));
            }
            (KeyModifiers::NONE, KeyCode::Char(']')) => {
                let page_len = self.page_len() as u8;
                self.page_index = (self.page_index + 1) % page_len;
            }
            (KeyModifiers::NONE, KeyCode::Char('[')) => {
                let page_len = self.page_len() as u8;
                self.page_index = (self.page_index + page_len - 1) % page_len;
            }
            (KeyModifiers::NONE, KeyCode::Char('s')) => {
                return self.handle_input(InputEvent::NextSort);
            }
            (KeyModifiers::NONE, KeyCode::Char('m')) if self.title() == "Status" => {
                if let Some(mode) = self.config_state.current_mode() {
                    self.mode_popup = Some(ModeSwitchPopup::new(mode));
                }
            }
            (KeyModifiers::NONE, KeyCode::Char('r')) if self.title() == "Status" => {
                return Ok(Some(Action::FetchConfigs));
            }
            (_, KeyCode::Char('R')) if self.title() == "Status" => {
                return Ok(Some(Action::ReloadConfigs));
            }
            (KeyModifiers::NONE, KeyCode::Char('k')) if self.title() == "Conns" => {
                let id = self
                    .con_state
                    .current_item_index()
                    .and_then(|index| self.con_state.get(index))
                    .map(|connection| connection.connection.id.clone());
                match id {
                    Some(id) => return Ok(Some(Action::CloseConnection { id })),
                    None => {
                        self.notice_popup = Some(NoticePopup::new(
                            "No Connection Selected",
                            "Hold the list with Space and move to a connection first"
                                .to_owned(),
                        ));
                    }
                }
            }
            (_, KeyCode::Char('K')) if self.title() == "Conns" => {
                if self.con_state.is_empty() {
                    self.notice_popup = Some(NoticePopup::new(
                        "No Active Connections",
                        "There is nothing to close".to_owned(),
                    ));
                } else {
                    self.confirm_popup = Some(ConfirmPopup {
                        title: "Close All Connections".to_owned(),
                        body: format!(
                            "Close all {} active connections?",
                            self.con_state.len()
                        ),
                        action: Action::CloseAllConnections,
                    });
                }
            }
            (KeyModifiers::NONE, KeyCode::Char('g')) if self.title() == "Status" => {
                // Immediate feedback: the server downloads the databases
                // before answering, which can take a long while.
                self.notice_popup = Some(NoticePopup::new(
                    "Updating Geo",
                    "Requested geo database update, this may take a while...".to_owned(),
                ));
                return Ok(Some(Action::UpdateGeo));
            }
            (KeyModifiers::NONE, KeyCode::Char('p')) => match self.title() {
                "Conns" => {
                    self.con_state.toggle_paused();
                }
                "Logs" => {
                    self.log_state.toggle_paused();
                }
                _ => {}
            },
            _ => {}
        }

        Ok(None)
    }

    fn open_server_popup(&mut self) {
        let config = get_config();
        let servers = config.servers.clone();
        let active = config.using.clone();
        drop(config);
        self.open_server_popup_with(servers, active);
        if let Some(popup) = self.server_popup.as_mut() {
            if popup.servers.is_empty() {
                popup.message = Some("No servers configured, press a to add one".to_owned());
            }
        }
    }

    pub fn open_server_popup_with(&mut self, servers: Vec<Server>, active: Option<Url>) {
        let index = active
            .as_ref()
            .and_then(|url| servers.iter().position(|server| &server.url == url))
            .unwrap_or(0);
        self.server_popup = Some(ServerSwitchPopup {
            servers,
            active,
            index,
            mode: ServerPopupMode::List,
            message: None,
        });
    }

    fn handle_server_popup_key(&mut self, event: crate::ListEvent) {
        let Some(popup) = self.server_popup.as_mut() else {
            return;
        };
        match &mut popup.mode {
            ServerPopupMode::List => {
                let len = popup.servers.len();
                if len == 0 {
                    return;
                }
                match event.code {
                    KeyCode::Up => popup.index = (popup.index + len - 1) % len,
                    KeyCode::Down => popup.index = (popup.index + 1) % len,
                    KeyCode::Enter => {
                        let popup = self.server_popup.take().expect("checked above");
                        let server = &popup.servers[popup.index];
                        // Re-selecting the active server is a no-op
                        if popup.active.as_ref() != Some(&server.url) {
                            self.switch_to_server = Some(server.url.clone());
                            self.should_quit = true;
                        }
                    }
                    _ => {}
                }
            }
            ServerPopupMode::ConfirmDelete => {
                if event.code == KeyCode::Enter {
                    Self::delete_selected_server(popup);
                }
            }
            ServerPopupMode::Add(form) => match event.code {
                KeyCode::Down => form.field = form.field.next(),
                KeyCode::Up => form.field = form.field.prev(),
                KeyCode::Left | KeyCode::Right if form.field == ServerFormField::Kind => {
                    form.kind = match form.kind {
                        ControllerKind::Mihomo => ControllerKind::Clash,
                        ControllerKind::Clash => ControllerKind::Mihomo,
                    };
                }
                KeyCode::Enter => Self::submit_server_form(popup),
                _ => {}
            },
        }
    }

    fn handle_server_popup_char(&mut self, event: KeyEvent) {
        let Some(popup) = self.server_popup.as_mut() else {
            return;
        };
        match &mut popup.mode {
            ServerPopupMode::List => match event.code {
                KeyCode::Char('a') => {
                    popup.message = None;
                    popup.mode = ServerPopupMode::Add(ServerForm::default());
                }
                KeyCode::Char('d') => {
                    if popup.servers.is_empty() {
                        return;
                    }
                    let selected = &popup.servers[popup.index];
                    if popup.active.as_ref() == Some(&selected.url) {
                        popup.message =
                            Some("Cannot delete the active server, switch away first".to_owned());
                    } else {
                        popup.message = None;
                        popup.mode = ServerPopupMode::ConfirmDelete;
                    }
                }
                _ => {}
            },
            ServerPopupMode::ConfirmDelete => {}
            ServerPopupMode::Add(form) => {
                let input = match form.field {
                    ServerFormField::Url => Some(&mut form.url),
                    ServerFormField::Secret => Some(&mut form.secret),
                    ServerFormField::Kind => None,
                };
                match (event.modifiers, event.code) {
                    (_, KeyCode::Tab) => form.field = form.field.next(),
                    (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
                        if let Some(input) = input {
                            input.clear();
                        }
                    }
                    (_, KeyCode::Backspace) => {
                        if let Some(input) = input {
                            input.pop();
                        }
                    }
                    (_, KeyCode::Char(' ')) if form.field == ServerFormField::Kind => {
                        form.kind = match form.kind {
                            ControllerKind::Mihomo => ControllerKind::Clash,
                            ControllerKind::Clash => ControllerKind::Mihomo,
                        };
                    }
                    (KeyModifiers::NONE | KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
                        if let Some(input) = input {
                            input.push(ch);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn submit_server_form(popup: &mut ServerSwitchPopup) {
        let ServerPopupMode::Add(form) = &mut popup.mode else {
            return;
        };
        let url = match Url::parse(form.url.trim()) {
            Ok(url) => url,
            Err(e) => {
                form.error = Some(format!("Invalid URL: {e}"));
                form.field = ServerFormField::Url;
                return;
            }
        };
        if popup.servers.iter().any(|server| server.url == url) {
            form.error = Some(format!("{url} is already configured"));
            form.field = ServerFormField::Url;
            return;
        }
        let server = Server {
            url,
            secret: match form.secret.trim() {
                "" => None,
                secret => Some(secret.to_owned()),
            },
            kind: form.kind,
        };
        if let Err(e) = commit_server_add(&server) {
            form.error = Some(e);
            return;
        }
        popup.message = Some(format!("Added {}", server.url));
        popup.servers.push(server);
        popup.mode = ServerPopupMode::List;
    }

    fn delete_selected_server(popup: &mut ServerSwitchPopup) {
        let url = popup.servers[popup.index].url.clone();
        if let Err(e) = commit_server_delete(&url) {
            popup.message = Some(e);
            popup.mode = ServerPopupMode::List;
            return;
        }
        popup.servers.remove(popup.index);
        if popup.index >= popup.servers.len() {
            popup.index = popup.servers.len().saturating_sub(1);
        }
        popup.message = Some(format!("Removed {url}"));
        popup.mode = ServerPopupMode::List;
    }

    fn pick_api_param(&mut self) -> bool {
        let Some(label) = self
            .active_api_state()
            .and_then(api::current_param_label)
        else {
            return false;
        };
        let value = match label {
            "group" if !self.proxy_tree.is_empty() => self.proxy_tree.current_group().name(),
            "proxy" if !self.proxy_tree.is_empty() => {
                match self.proxy_tree.current_group().current_member_name() {
                    Some(name) => name,
                    None => return false,
                }
            }
            "connection id" => {
                match self
                    .con_state
                    .current_item_index()
                    .and_then(|index| self.con_state.get(index))
                {
                    Some(connection) => connection.connection.id.as_str(),
                    None => return false,
                }
            }
            _ => return false,
        };
        let value = value.to_owned();
        self.active_api_state_mut()
            .map(|state| api::set_current_param_value(state, &value))
            .unwrap_or(false)
    }

    pub const fn debug_page_index(&self) -> u8 {
        Self::TITLES.len() as u8 - 1
    }

    pub const fn api_page_index() -> u8 {
        7
    }

    fn drop_events(&mut self, num: usize) {
        let num = num.min(self.debug_state.len());
        self.debug_state.drain(..num).for_each(drop);
    }
}

fn commit_server_add(server: &Server) -> Result<(), String> {
    let mut config = crate::ui::get_config_mut();
    config.servers.push(server.clone());
    config
        .write()
        .map_err(|e| format!("Failed to save config: {e}"))
}

fn commit_server_delete(url: &Url) -> Result<(), String> {
    let mut config = crate::ui::get_config_mut();
    config.servers.retain(|server| &server.url != url);
    config
        .write()
        .map_err(|e| format!("Failed to save config: {e}"))
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::TuiStates;

    use crate::{
        mihomoctl::model::{ConnectionWithSpeed, ConnectionsWithSpeed, Level, Log},
        interactive::{Config, ConSort, ControllerKind},
        ui::api::{self, ApiOperation},
        ui::components::MovableListState,
        ui::config::init_config,
        Action, Event, UpdateEvent,
    };

    fn key(code: KeyCode) -> Event {
        Event::from(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl_key(code: KeyCode) -> Event {
        Event::from(KeyEvent::new(code, KeyModifiers::CONTROL))
    }

    fn connections(upload_total: u64, download_total: u64) -> ConnectionsWithSpeed {
        ConnectionsWithSpeed {
            connections: Vec::new(),
            upload_total,
            download_total,
        }
    }

    fn log(payload: &str) -> Log {
        Log {
            log_type: Level::Info,
            payload: payload.to_owned(),
        }
    }

    #[test]
    fn status_page_m_key_opens_mode_switcher_popup() {
        use mihomoctl_core::model::Mode;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-mode-switch-test.ron").unwrap());
        let mut state = TuiStates::default();
        assert_eq!(state.title(), "Status");

        // Mode unknown yet: key press is a no-op
        state.handle(key(KeyCode::Char('m'))).unwrap();
        assert!(state.mode_popup.is_none());

        let config: crate::mihomoctl::model::Config = mihomoctl_core::serde_json::from_str(
            r#"{
                "port": 7890, "socks-port": 7891, "redir-port": 0, "tproxy-port": 0,
                "mixed-port": 7893, "allow-lan": false, "ipv6": false,
                "mode": "rule", "log-level": "info", "bind-address": "*"
            }"#,
        )
        .unwrap();
        state
            .handle(Event::Update(UpdateEvent::Config(config)))
            .unwrap();

        state.handle(key(KeyCode::Char('m'))).unwrap();
        let popup = state.mode_popup.as_ref().expect("popup should open");
        // Preselects the current mode
        assert_eq!(popup.selected(), Mode::Rule);

        // Selecting the current mode again closes without emitting an action
        let action = state.handle(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, None);
        assert!(state.mode_popup.is_none());

        // Move to Global and apply
        state.handle(key(KeyCode::Char('m'))).unwrap();
        state.handle(key(KeyCode::Down)).unwrap();
        let action = state.handle(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(Action::SetMode { mode: Mode::Global }));
        assert!(state.mode_popup.is_none());

        // Esc closes without action
        state.handle(key(KeyCode::Char('m'))).unwrap();
        let action = state.handle(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, None);
        assert!(state.mode_popup.is_none());
    }

    #[test]
    fn status_page_r_keys_fetch_or_reload_configs() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-config-reload-test.ron").unwrap());
        let mut state = TuiStates::default();
        assert_eq!(state.title(), "Status");

        // r fetches the remote runtime configs
        let action = state.handle(key(KeyCode::Char('r'))).unwrap();
        assert_eq!(action, Some(Action::FetchConfigs));

        // Shift-R asks mihomo to reload the config file from disk
        let action = state
            .handle(Event::from(KeyEvent::new(
                KeyCode::Char('R'),
                KeyModifiers::SHIFT,
            )))
            .unwrap();
        assert_eq!(action, Some(Action::ReloadConfigs));

        // Only active on the Status page
        state.handle(key(KeyCode::Char('4'))).unwrap();
        let action = state.handle(key(KeyCode::Char('r'))).unwrap();
        assert_eq!(action, None);
    }

    #[test]
    fn status_page_g_key_requests_geo_update() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-geo-update-test.ron").unwrap());
        let mut state = TuiStates::default();
        assert_eq!(state.title(), "Status");

        let action = state.handle(key(KeyCode::Char('g'))).unwrap();
        assert_eq!(action, Some(Action::UpdateGeo));
        // Immediate feedback while the server downloads the databases
        assert!(state.notice_popup.is_some());

        // The pending notice absorbs the next key press
        state.handle(key(KeyCode::Char('4'))).unwrap();
        assert!(state.notice_popup.is_none());

        // Only active on the Status page
        state.handle(key(KeyCode::Char('4'))).unwrap();
        let action = state.handle(key(KeyCode::Char('g'))).unwrap();
        assert_eq!(action, None);
    }

    #[test]
    fn geo_update_result_shows_notice_or_error_popup() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-geo-popup-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::GeoUpdateResult { error: None }))
            .unwrap();
        assert!(state.notice_popup.is_some());

        state
            .handle(Event::Update(UpdateEvent::GeoUpdateResult {
                error: Some("download failed".to_owned()),
            }))
            .unwrap();
        assert!(state.api_result_popup.is_some());
    }

    fn connection_fixture(id: &str) -> ConnectionWithSpeed {
        let connection = mihomoctl_core::serde_json::from_value(
            mihomoctl_core::serde_json::json!({
                "id": id,
                "upload": 0,
                "download": 0,
                "metadata": {
                    "type": "HTTP",
                    "sourceIP": "127.0.0.1",
                    "sourcePort": "50000",
                    "destinationIP": "1.1.1.1",
                    "destinationPort": "443",
                    "host": "example.com",
                    "network": "tcp"
                },
                "rule": "Match",
                "rulePayload": "",
                "start": "2026-01-01T00:00:00Z",
                "chains": ["DIRECT"]
            }),
        )
        .unwrap();
        ConnectionWithSpeed {
            connection,
            upload: None,
            download: None,
        }
    }

    #[test]
    fn conns_page_k_key_closes_selected_connection() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-conn-kill-test.ron").unwrap());
        let mut state = TuiStates::default();

        // Go to Conns page
        state.handle(key(KeyCode::Char('4'))).unwrap();
        assert_eq!(state.title(), "Conns");

        // Empty list: no action, but a hint notice appears
        let action = state.handle(key(KeyCode::Char('k'))).unwrap();
        assert_eq!(action, None);
        assert!(state.notice_popup.take().is_some());

        let update = ConnectionsWithSpeed {
            connections: vec![connection_fixture("conn-1"), connection_fixture("conn-2")],
            upload_total: 0,
            download_total: 0,
        };
        state
            .handle(Event::Update(UpdateEvent::Connection(update)))
            .unwrap();

        let action = state.handle(key(KeyCode::Char('k'))).unwrap();
        assert!(matches!(action, Some(Action::CloseConnection { .. })));
    }

    #[test]
    fn conns_page_shift_k_asks_before_closing_all_connections() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-conn-kill-all-test.ron").unwrap());
        let mut state = TuiStates::default();

        state.handle(key(KeyCode::Char('4'))).unwrap();
        assert_eq!(state.title(), "Conns");

        // Empty list: hint only
        state
            .handle(Event::from(KeyEvent::new(
                KeyCode::Char('K'),
                KeyModifiers::SHIFT,
            )))
            .unwrap();
        assert!(state.confirm_popup.is_none());
        assert!(state.notice_popup.take().is_some());

        let update = ConnectionsWithSpeed {
            connections: vec![connection_fixture("conn-1")],
            upload_total: 0,
            download_total: 0,
        };
        state
            .handle(Event::Update(UpdateEvent::Connection(update)))
            .unwrap();

        // Shift-K opens the confirm popup
        state
            .handle(Event::from(KeyEvent::new(
                KeyCode::Char('K'),
                KeyModifiers::SHIFT,
            )))
            .unwrap();
        assert!(state.confirm_popup.is_some());

        // Esc cancels
        let action = state.handle(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, None);
        assert!(state.confirm_popup.is_none());

        // Open again and confirm with Enter
        state
            .handle(Event::from(KeyEvent::new(
                KeyCode::Char('K'),
                KeyModifiers::SHIFT,
            )))
            .unwrap();
        let action = state.handle(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(Action::CloseAllConnections));
        assert!(state.confirm_popup.is_none());
    }

    #[test]
    fn connection_close_result_shows_notice_or_error_popup() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-conn-popup-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::ConnectionCloseResult {
                all: false,
                error: None,
            }))
            .unwrap();
        assert!(state.notice_popup.take().is_some());

        state
            .handle(Event::Update(UpdateEvent::ConnectionCloseResult {
                all: true,
                error: Some("connection not found".to_owned()),
            }))
            .unwrap();
        assert!(state.api_result_popup.is_some());
    }

    #[test]
    fn config_fetch_result_shows_notice_or_error_popup() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-fetch-popup-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::ConfigFetchResult {
                error: None,
            }))
            .unwrap();
        assert!(state.notice_popup.is_some());

        state
            .handle(Event::Update(UpdateEvent::ConfigFetchResult {
                error: Some("connection refused".to_owned()),
            }))
            .unwrap();
        assert!(state.api_result_popup.is_some());
    }

    #[test]
    fn config_reload_result_shows_notice_or_error_popup() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-reload-popup-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::ConfigReloadResult {
                error: None,
            }))
            .unwrap();
        assert!(state.notice_popup.is_some());
        assert!(state.api_result_popup.is_none());

        state
            .handle(Event::Update(UpdateEvent::ConfigReloadResult {
                error: Some("config file not found".to_owned()),
            }))
            .unwrap();
        assert!(state.api_result_popup.is_some());
    }

    #[test]
    fn mode_switcher_selection_wraps_around() {
        use mihomoctl_core::model::Mode;

        use super::ModeSwitchPopup;

        let mut popup = ModeSwitchPopup::new(Mode::Rule);
        assert_eq!(popup.index, 0);

        popup.index = ModeSwitchPopup::MODES.len() - 1;
        assert_eq!(popup.selected(), Mode::Direct);
    }

    #[test]
    fn mode_switch_result_shows_notice_or_error_popup() {
        use mihomoctl_core::model::Mode;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-mode-popup-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::ModeSwitchResult {
                mode: Mode::Global,
                error: None,
            }))
            .unwrap();
        assert!(state.notice_popup.is_some());
        assert!(state.api_result_popup.is_none());

        state
            .handle(Event::Update(UpdateEvent::ModeSwitchResult {
                mode: Mode::Direct,
                error: Some("connection refused".to_owned()),
            }))
            .unwrap();
        assert!(state.api_result_popup.is_some());
    }

    #[test]
    fn api_page_enter_invokes_current_endpoint() {
        let state = api::default_api_state();
        let action = api::current_operation(&state).map(|operation| Action::InvokeApi {
            operation,
            params: Default::default(),
        });

        assert_eq!(
            action,
            Some(Action::InvokeApi {
                operation: ApiOperation::Version,
                params: Default::default(),
            })
        );
    }

    #[test]
    fn dangerous_api_requires_second_enter_to_confirm() {
        let mut state = api::default_api_state();
        api::select_operation(&mut state, ApiOperation::Restart).unwrap();

        let first = api::submit_current(&mut state);
        assert_eq!(first, None);
        assert!(
            api::current_result(&state)
                .unwrap()
                .contains("press Enter again")
        );

        let second = api::submit_current(&mut state);
        assert_eq!(
            second,
            Some(Action::InvokeApi {
                operation: ApiOperation::Restart,
                params: Default::default(),
            })
        );
    }

    #[test]
    fn tui_exposes_split_api_control_pages() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-api-pages-test.ron").unwrap());
        assert!(!TuiStates::TITLES.contains(&"Runtime"));
        assert!(TuiStates::TITLES.contains(&"Core"));
        assert!(!TuiStates::TITLES.contains(&"DNS/Storage/Debug"));
        assert!(TuiStates::TITLES.contains(&"DNS"));
        assert!(!TuiStates::TITLES.contains(&"Configs"));

        let mut state = TuiStates::default();
        state.page_index = 5;
        assert_eq!(
            api::current_operation(state.active_api_state().unwrap()),
            Some(ApiOperation::GetConfigs)
        );
        state.page_index = 6;
        assert_eq!(
            api::current_operation(state.active_api_state().unwrap()),
            Some(ApiOperation::FlushFakeIpCache)
        );
        state.page_index = 7;
        assert_eq!(
            api::current_operation(state.active_api_state().unwrap()),
            Some(ApiOperation::Version)
        );
    }

    #[test]
    fn tui_state_uses_controller_kind_for_api_catalogs() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-kind-state-test.ron").unwrap());
        let clash = TuiStates::for_controller_kind(ControllerKind::Clash);
        let clash_api = clash
            .api_state
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();
        let clash_dns = clash
            .dns_api_state
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();

        assert!(!clash_api.contains(&ApiOperation::Memory));
        assert!(!clash_api.contains(&ApiOperation::FlushFakeIpCache));
        assert!(clash_api.contains(&ApiOperation::Version));
        assert_eq!(clash_dns, vec![ApiOperation::DnsQuery]);

        let mihomo = TuiStates::for_controller_kind(ControllerKind::Mihomo);
        let mihomo_api = mihomo
            .api_state
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();
        assert!(mihomo_api.contains(&ApiOperation::Memory));
        assert_eq!(
            api::current_operation(&mihomo.dns_api_state),
            Some(ApiOperation::FlushFakeIpCache)
        );
    }

    #[test]
    fn api_control_page_uses_arrow_selected_operation() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-api-selection-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 5;

        state.handle(key(KeyCode::Down)).unwrap();
        assert_eq!(
            api::current_operation(state.active_api_state().unwrap()),
            Some(ApiOperation::ReloadConfigs)
        );

        assert_eq!(state.handle(key(KeyCode::Enter)).unwrap(), None);
        assert!(api::is_editing(state.active_api_state().unwrap()));
        assert_eq!(
            state.handle(key(KeyCode::Enter)).unwrap(),
            None
        );
        assert!(
            api::current_result(state.active_api_state().unwrap())
                .unwrap()
                .contains("press Enter again")
        );
        assert_eq!(
            state.handle(key(KeyCode::Enter)).unwrap(),
            Some(Action::InvokeApi {
                operation: ApiOperation::ReloadConfigs,
                params: Default::default(),
            })
        );
    }

    #[test]
    fn api_result_update_opens_popup_and_escape_closes_it() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-api-popup-test.ron").unwrap());
        let mut state = TuiStates::default();
        state
            .handle(Event::Update(UpdateEvent::ApiResult {
                operation: ApiOperation::DnsQuery,
                result: r#"{"Answer":[]}"#.to_owned(),
            }))
            .unwrap();

        let popup = state.api_result_popup.as_ref().unwrap();
        assert!(popup.title.contains("query example.com A"));
        assert!(popup.body.contains("Answer"));

        state.handle(key(KeyCode::Esc)).unwrap();
        assert!(state.api_result_popup.is_none());
    }

    #[test]
    fn api_input_box_only_opens_for_parameterized_operations() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-api-input-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 6;

        state.handle(key(KeyCode::Char('i'))).unwrap();
        assert!(!api::is_editing(state.active_api_state().unwrap()));

        state.handle(key(KeyCode::Down)).unwrap();
        state.handle(key(KeyCode::Down)).unwrap();
        assert_eq!(
            api::current_operation(state.active_api_state().unwrap()),
            Some(ApiOperation::DnsQuery)
        );

        state.handle(key(KeyCode::Char('i'))).unwrap();
        assert!(api::is_editing(state.active_api_state().unwrap()));

        state.handle(key(KeyCode::Down)).unwrap();
        assert_eq!(
            api::current_operation(state.active_api_state().unwrap()),
            Some(ApiOperation::DnsQuery)
        );

        state.handle(key(KeyCode::Esc)).unwrap();
        assert!(!api::is_editing(state.active_api_state().unwrap()));
    }

    #[test]
    fn dns_query_enter_prompts_for_domain_before_running() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-dns-input-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 6;
        state.handle(key(KeyCode::Down)).unwrap();
        state.handle(key(KeyCode::Down)).unwrap();

        assert_eq!(
            state.handle(key(KeyCode::Enter)).unwrap(),
            None
        );
        assert!(api::is_editing(state.active_api_state().unwrap()));
        assert_eq!(
            api::current_param_value(state.active_api_state().unwrap()),
            Some("example.com")
        );

        state.handle(ctrl_key(KeyCode::Char('u'))).unwrap();
        assert_eq!(api::current_param_value(state.active_api_state().unwrap()), Some(""));

        for ch in "openai.com".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }

        assert_eq!(
            state.handle(key(KeyCode::Enter)).unwrap(),
            Some(Action::InvokeApi {
                operation: ApiOperation::DnsQuery,
                params: api::ApiParams {
                    dns_name: "openai.com".to_owned(),
                    ..Default::default()
                },
            })
        );
    }

    #[test]
    fn dns_query_prompts_again_after_first_result() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-dns-repeat-input-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 6;
        state.handle(key(KeyCode::Down)).unwrap();
        state.handle(key(KeyCode::Down)).unwrap();

        state.handle(key(KeyCode::Enter)).unwrap();
        state.handle(ctrl_key(KeyCode::Char('u'))).unwrap();
        for ch in "openai.com".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }
        assert!(state.handle(key(KeyCode::Enter)).unwrap().is_some());
        state
            .handle(Event::Update(UpdateEvent::ApiResult {
                operation: ApiOperation::DnsQuery,
                result: "{}".to_owned(),
            }))
            .unwrap();
        state.handle(key(KeyCode::Esc)).unwrap();

        assert_eq!(state.handle(key(KeyCode::Enter)).unwrap(), None);
        assert!(api::is_editing(state.active_api_state().unwrap()));
        assert_eq!(
            api::current_param_value(state.active_api_state().unwrap()),
            Some("openai.com")
        );
    }

    #[test]
    fn api_result_popup_arrow_keys_scroll_result() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-api-popup-scroll-test.ron").unwrap());
        let mut state = TuiStates::default();
        state
            .handle(Event::Update(UpdateEvent::ApiResult {
                operation: ApiOperation::DnsQuery,
                result: "line1\nline2\nline3".to_owned(),
            }))
            .unwrap();

        state.handle(key(KeyCode::Down)).unwrap();
        assert_eq!(state.api_result_popup.as_ref().unwrap().offset.y, 1);

        state.handle(key(KeyCode::Right)).unwrap();
        assert_eq!(state.api_result_popup.as_ref().unwrap().offset.x, 4);
    }

    #[test]
    fn bracket_keys_cycle_visible_pages() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-page-cycle-test.ron").unwrap());
        let mut state = TuiStates::default();

        for _ in 0..5 {
            state.handle(key(KeyCode::Char(']'))).unwrap();
        }
        assert_eq!(state.title(), "Core");

        for _ in 0..3 {
            state.handle(key(KeyCode::Char(']'))).unwrap();
        }
        assert_eq!(state.title(), "Status");

        state.handle(key(KeyCode::Char(']'))).unwrap();
        assert_eq!(state.title(), "Proxies");

        state.handle(key(KeyCode::Char('['))).unwrap();
        assert_eq!(state.title(), "Status");
    }

    #[test]
    fn connection_page_uses_real_sort_method() {
        let state = MovableListState::<ConnectionWithSpeed, ConSort>::default();
        assert_eq!(state.sort_label(), "Time ▼");
    }

    #[test]
    fn debug_events_are_trimmed_when_limit_is_reached() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-debug-trim-test.ron").unwrap());
        let mut state = TuiStates::default();

        for _ in 0..350 {
            state.handle(Event::Quit).unwrap();
        }

        assert!(state.debug_state.len() <= 300);
        assert_eq!(state.all_events_recv, 350);
    }

    #[test]
    fn slash_starts_search_on_connection_rule_and_log_pages() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-list-search-test.ron").unwrap());
        let mut state = TuiStates::default();

        state.page_index = 3;
        state.handle(key(KeyCode::Char('/'))).unwrap();
        assert!(state.con_state.is_searching());

        state.page_index = 2;
        state.handle(key(KeyCode::Char('/'))).unwrap();
        assert!(state.rule_state.is_searching());

        state.page_index = 4;
        state.handle(key(KeyCode::Char('/'))).unwrap();
        assert!(state.log_state.is_searching());
    }

    #[test]
    fn esc_and_enter_exit_list_search() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-list-search-exit-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 4;

        state.handle(key(KeyCode::Char('/'))).unwrap();
        state.handle(key(KeyCode::Char('e'))).unwrap();
        assert_eq!(state.log_state.search_query(), Some("e"));

        state.handle(key(KeyCode::Esc)).unwrap();
        assert!(!state.log_state.is_searching());

        state.handle(key(KeyCode::Char('/'))).unwrap();
        state.handle(key(KeyCode::Enter)).unwrap();
        assert!(!state.log_state.is_searching());
    }

    #[test]
    fn p_pauses_and_resumes_connection_updates() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-connections-pause-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 3;

        state
            .handle(Event::Update(UpdateEvent::Connection(connections(10, 20))))
            .unwrap();
        assert_eq!(state.con_size, (10, 20));

        state.handle(key(KeyCode::Char('p'))).unwrap();
        assert!(state.con_state.is_paused());

        state
            .handle(Event::Update(UpdateEvent::Connection(connections(30, 40))))
            .unwrap();
        assert_eq!(state.con_size, (10, 20));

        state.handle(key(KeyCode::Char('p'))).unwrap();
        assert!(!state.con_state.is_paused());

        state
            .handle(Event::Update(UpdateEvent::Connection(connections(50, 60))))
            .unwrap();
        assert_eq!(state.con_size, (50, 60));
    }

    fn test_servers() -> Vec<crate::interactive::Server> {
        use crate::interactive::Server;
        vec![
            Server {
                url: url::Url::parse("http://127.0.0.1:9090/").unwrap(),
                secret: None,
                kind: ControllerKind::Mihomo,
            },
            Server {
                url: url::Url::parse("http://10.0.0.2:9090/").unwrap(),
                secret: None,
                kind: ControllerKind::Clash,
            },
        ]
    }

    #[test]
    fn server_popup_starts_on_active_server_and_esc_closes_it() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-popup-test.ron").unwrap());
        let mut state = TuiStates::default();
        let servers = test_servers();
        let active = Some(servers[1].url.clone());

        state.open_server_popup_with(servers, active);
        assert_eq!(state.server_popup.as_ref().unwrap().index, 1);

        state.handle(key(KeyCode::Esc)).unwrap();
        assert!(state.server_popup.is_none());
        assert!(state.switch_to_server.is_none());
        assert!(!state.should_quit);
    }

    #[test]
    fn server_popup_enter_requests_switch_to_selected_server() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-switch-test.ron").unwrap());
        let mut state = TuiStates::default();
        let servers = test_servers();
        let expected = servers[1].url.clone();

        state.open_server_popup_with(servers, Some("http://127.0.0.1:9090/".parse().unwrap()));
        state.handle(key(KeyCode::Down)).unwrap();
        state.handle(key(KeyCode::Enter)).unwrap();

        assert!(state.server_popup.is_none());
        assert_eq!(state.switch_to_server, Some(expected));
        assert!(state.should_quit);
    }

    #[test]
    fn server_popup_enter_on_active_server_is_noop() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-noop-test.ron").unwrap());
        let mut state = TuiStates::default();
        let servers = test_servers();
        let active = Some(servers[0].url.clone());

        state.open_server_popup_with(servers, active);
        state.handle(key(KeyCode::Enter)).unwrap();

        assert!(state.server_popup.is_none());
        assert!(state.switch_to_server.is_none());
        assert!(!state.should_quit);
    }

    #[test]
    fn server_popup_navigation_wraps_and_blocks_other_keys() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-nav-test.ron").unwrap());
        let mut state = TuiStates::default();

        state.open_server_popup_with(test_servers(), None);
        assert_eq!(state.server_popup.as_ref().unwrap().index, 0);

        state.handle(key(KeyCode::Up)).unwrap();
        assert_eq!(state.server_popup.as_ref().unwrap().index, 1);

        state.handle(key(KeyCode::Down)).unwrap();
        assert_eq!(state.server_popup.as_ref().unwrap().index, 0);

        // Page shortcuts are ignored while the popup is open
        state.handle(key(KeyCode::Char('3'))).unwrap();
        assert_eq!(state.page_index, 0);
    }

    #[test]
    fn shift_t_tests_all_unique_normal_nodes_across_groups() {
        use std::collections::HashMap;

        use crate::mihomoctl::model::{Proxies, Proxy, ProxyType};

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-test-all-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 1; // Proxies page

        let proxy = |proxy_type: ProxyType, all: Option<Vec<&str>>, now: Option<&str>| Proxy {
            proxy_type,
            history: vec![],
            udp: None,
            all: all.map(|x| x.into_iter().map(ToOwned::to_owned).collect()),
            now: now.map(ToOwned::to_owned),
        };
        let mut map = HashMap::new();
        map.insert("a".to_owned(), proxy(ProxyType::Vmess, None, None));
        map.insert("b".to_owned(), proxy(ProxyType::Trojan, None, None));
        map.insert(
            "G1".to_owned(),
            proxy(ProxyType::Selector, Some(vec!["a", "b"]), Some("a")),
        );
        map.insert(
            "G2".to_owned(),
            proxy(ProxyType::Selector, Some(vec!["b", "G1"]), Some("b")),
        );
        state
            .handle(Event::Update(UpdateEvent::Proxies(Proxies {
                proxies: map,
            })))
            .unwrap();

        let shift_t = || Event::from(KeyEvent::new(KeyCode::Char('T'), KeyModifiers::SHIFT));
        let action = state.handle(shift_t()).unwrap();
        let Some(Action::TestLatency { mut proxies }) = action else {
            panic!("expected TestLatency action, got {action:?}");
        };
        proxies.sort();
        // Every normal node exactly once; nested groups are not tested directly
        assert_eq!(proxies, vec!["a".to_owned(), "b".to_owned()]);
        assert!(state.proxy_tree.is_testing());

        // No double-trigger while a test is already running
        assert_eq!(state.handle(shift_t()).unwrap(), None);
    }

    #[test]
    fn shift_h_toggles_scrollable_help_popup() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-help-test.ron").unwrap());
        let mut state = TuiStates::default();
        let shift_h = || Event::from(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT));

        state.handle(shift_h()).unwrap();
        assert!(state.help_popup.is_some());

        state.handle(key(KeyCode::Down)).unwrap();
        assert_eq!(state.help_popup.as_ref().unwrap().y, 1);

        // Other keys are swallowed while help is open ('q' does not quit)
        state.handle(key(KeyCode::Char('q'))).unwrap();
        assert!(!state.should_quit);
        assert!(state.help_popup.is_some());

        state.handle(shift_h()).unwrap();
        assert!(state.help_popup.is_none());

        state.handle(shift_h()).unwrap();
        state.handle(key(KeyCode::Esc)).unwrap();
        assert!(state.help_popup.is_none());
    }

    #[test]
    fn notice_popup_closes_on_any_key_without_side_effects() {
        use super::NoticePopup;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-notice-key-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.notice_popup = Some(NoticePopup::new("Server Switched", "Switched"));

        // 'q' only dismisses the notice instead of quitting
        state.handle(key(KeyCode::Char('q'))).unwrap();
        assert!(state.notice_popup.is_none());
        assert!(!state.should_quit);
    }

    #[test]
    fn notice_popup_expires_on_background_updates() {
        use std::time::{Duration, Instant};

        use super::NoticePopup;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-notice-ttl-test.ron").unwrap());
        let mut state = TuiStates::default();
        let mut notice = NoticePopup::new("Server Switched", "Switched");
        notice.created = Instant::now() - NoticePopup::TTL - Duration::from_secs(1);
        state.notice_popup = Some(notice);

        state
            .handle(Event::Update(UpdateEvent::Connection(connections(1, 2))))
            .unwrap();

        assert!(state.notice_popup.is_none());
    }

    #[test]
    fn server_popup_supports_adding_from_an_empty_list() {
        use super::ServerPopupMode;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-empty-test.ron").unwrap());
        let mut state = TuiStates::default();

        state.open_server_popup_with(Vec::new(), None);
        let popup = state.server_popup.as_ref().unwrap();
        assert!(popup.servers.is_empty());

        // Navigation and delete are no-ops on an empty list
        state.handle(key(KeyCode::Down)).unwrap();
        state.handle(key(KeyCode::Char('d'))).unwrap();
        state.handle(key(KeyCode::Enter)).unwrap();
        assert!(matches!(
            state.server_popup.as_ref().unwrap().mode,
            ServerPopupMode::List
        ));
        assert!(!state.should_quit);

        // Adding works right away
        state.handle(key(KeyCode::Char('a'))).unwrap();
        for ch in "http://10.1.1.1:9090/".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }
        state.handle(key(KeyCode::Enter)).unwrap();

        let popup = state.server_popup.as_ref().unwrap();
        assert_eq!(popup.servers.len(), 1);
        assert_eq!(popup.servers[0].url.as_str(), "http://10.1.1.1:9090/");
    }

    #[test]
    fn server_popup_add_form_saves_new_server() {
        use super::{ServerFormField, ServerPopupMode};

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-add-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.open_server_popup_with(test_servers(), None);

        state.handle(key(KeyCode::Char('a'))).unwrap();
        assert!(matches!(
            state.server_popup.as_ref().unwrap().mode,
            ServerPopupMode::Add(_)
        ));

        for ch in "http://192.168.1.5:9090/".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }
        state.handle(key(KeyCode::Tab)).unwrap();
        for ch in "token".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }
        state.handle(key(KeyCode::Down)).unwrap(); // to kind field
        state.handle(key(KeyCode::Right)).unwrap(); // mihomo -> clash
        {
            let popup = state.server_popup.as_ref().unwrap();
            let ServerPopupMode::Add(form) = &popup.mode else {
                panic!("expected add mode");
            };
            assert_eq!(form.field, ServerFormField::Kind);
            assert_eq!(form.kind, ControllerKind::Clash);
        }

        state.handle(key(KeyCode::Enter)).unwrap();

        let popup = state.server_popup.as_ref().unwrap();
        assert!(matches!(popup.mode, ServerPopupMode::List));
        let added = popup.servers.last().unwrap();
        assert_eq!(added.url.as_str(), "http://192.168.1.5:9090/");
        assert_eq!(added.secret.as_deref(), Some("token"));
        assert_eq!(added.kind, ControllerKind::Clash);
        assert!(popup.message.as_ref().unwrap().contains("Added"));
    }

    #[test]
    fn server_popup_add_form_rejects_invalid_and_duplicate_urls() {
        use super::ServerPopupMode;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-invalid-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.open_server_popup_with(test_servers(), None);
        state.handle(key(KeyCode::Char('a'))).unwrap();

        for ch in "not a url".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }
        state.handle(key(KeyCode::Enter)).unwrap();
        {
            let popup = state.server_popup.as_ref().unwrap();
            let ServerPopupMode::Add(form) = &popup.mode else {
                panic!("expected add mode");
            };
            assert!(form.error.as_ref().unwrap().contains("Invalid URL"));
        }

        state.handle(ctrl_key(KeyCode::Char('u'))).unwrap();
        for ch in "http://127.0.0.1:9090/".chars() {
            state.handle(key(KeyCode::Char(ch))).unwrap();
        }
        state.handle(key(KeyCode::Enter)).unwrap();
        {
            let popup = state.server_popup.as_ref().unwrap();
            let ServerPopupMode::Add(form) = &popup.mode else {
                panic!("expected add mode");
            };
            assert!(form.error.as_ref().unwrap().contains("already configured"));
        }

        // Esc backs out to the list instead of closing the popup
        state.handle(key(KeyCode::Esc)).unwrap();
        let popup = state.server_popup.as_ref().unwrap();
        assert!(matches!(popup.mode, ServerPopupMode::List));
        assert_eq!(popup.servers.len(), 2);

        state.handle(key(KeyCode::Esc)).unwrap();
        assert!(state.server_popup.is_none());
    }

    #[test]
    fn server_popup_deletes_inactive_server_after_confirmation() {
        use super::ServerPopupMode;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-del-test.ron").unwrap());
        let mut state = TuiStates::default();
        let servers = test_servers();
        let active = Some(servers[0].url.clone());
        state.open_server_popup_with(servers, active);

        state.handle(key(KeyCode::Down)).unwrap();
        state.handle(key(KeyCode::Char('d'))).unwrap();
        assert!(matches!(
            state.server_popup.as_ref().unwrap().mode,
            ServerPopupMode::ConfirmDelete
        ));

        state.handle(key(KeyCode::Enter)).unwrap();

        let popup = state.server_popup.as_ref().unwrap();
        assert!(matches!(popup.mode, ServerPopupMode::List));
        assert_eq!(popup.servers.len(), 1);
        assert_eq!(popup.servers[0].url.as_str(), "http://127.0.0.1:9090/");
        assert!(popup.message.as_ref().unwrap().contains("Removed"));
    }

    #[test]
    fn server_popup_refuses_to_delete_the_active_server() {
        use super::ServerPopupMode;

        let _ = init_config(Config::from_dir("/tmp/mihomoctl-server-del-active-test.ron").unwrap());
        let mut state = TuiStates::default();
        let servers = test_servers();
        let active = Some(servers[0].url.clone());
        state.open_server_popup_with(servers, active);

        state.handle(key(KeyCode::Char('d'))).unwrap();

        let popup = state.server_popup.as_ref().unwrap();
        assert!(matches!(popup.mode, ServerPopupMode::List));
        assert_eq!(popup.servers.len(), 2);
        assert!(popup.message.as_ref().unwrap().contains("active"));
    }

    #[test]
    fn p_pauses_and_resumes_log_updates() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-logs-pause-test.ron").unwrap());
        let mut state = TuiStates::default();
        state.page_index = 4;

        state
            .handle(Event::Update(UpdateEvent::Log(log("first"))))
            .unwrap();
        assert_eq!(state.log_state.len(), 1);

        state.handle(key(KeyCode::Char('p'))).unwrap();
        assert!(state.log_state.is_paused());

        state
            .handle(Event::Update(UpdateEvent::Log(log("second"))))
            .unwrap();
        assert_eq!(state.log_state.len(), 1);

        state.handle(key(KeyCode::Char('p'))).unwrap();
        assert!(!state.log_state.is_paused());

        state
            .handle(Event::Update(UpdateEvent::Log(log("third"))))
            .unwrap();
        assert_eq!(state.log_state.len(), 2);
        assert_eq!(state.log_state[1].payload, "third");
    }

    #[test]
    fn successful_node_switch_shows_confirmation() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-node-switch-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::ProxySelectionResult {
                group: "GLOBAL".to_owned(),
                proxy: "node-a".to_owned(),
                error: None,
            }))
            .unwrap();

        let notice = state.notice_popup.as_ref().unwrap();
        assert_eq!(notice.title, "Node Switched");
        assert!(notice.body.contains("GLOBAL → node-a"));
    }

    #[test]
    fn failed_node_switch_shows_error_details() {
        let _ = init_config(Config::from_dir("/tmp/mihomoctl-node-switch-error-test.ron").unwrap());
        let mut state = TuiStates::default();

        state
            .handle(Event::Update(UpdateEvent::ProxySelectionResult {
                group: "GLOBAL".to_owned(),
                proxy: "node-a".to_owned(),
                error: Some("authentication failed".to_owned()),
            }))
            .unwrap();

        let popup = state.api_result_popup.as_ref().unwrap();
        assert_eq!(popup.title, "Switch Node Failed");
        assert!(popup.body.contains("authentication failed"));
    }
}
