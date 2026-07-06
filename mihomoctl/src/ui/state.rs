use std::{collections::HashMap, time::Instant};

use mihomoctl_core::{
    model::{ConnectionWithSpeed, Log, Rule, Traffic, Version},
    serde_json::Value,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use smart_default::SmartDefault;
use tui::{
    style::{Color, Style},
    text::{Span, Spans},
};

use crate::{
    interactive::{ConSort, ControllerKind, Noop, RuleSort},
    ui::{
        api::{self, ApiListState},
        components::{MovableListManage, MovableListManager, MovableListState, ProxyTree},
        TuiResult,
    },
    Action, ConfigState, Event, InputEvent, UpdateEvent,
};

pub(crate) type LogListState<'a> = MovableListState<'a, Log, Noop>;
pub(crate) type ConListState<'a> = MovableListState<'a, ConnectionWithSpeed, ConSort>;
pub(crate) type RuleListState<'a> = MovableListState<'a, Rule, RuleSort>;
pub(crate) type DebugListState<'a> = MovableListState<'a, Event, Noop>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResultPopup {
    pub title: String,
    pub body: String,
    pub offset: crate::Coord,
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
        "Configs",
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
            Event::Input(event) => self.handle_input(event),
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
                    (KeyModifiers::NONE, KeyCode::Char(ch)) => {
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
            (KeyModifiers::NONE, KeyCode::Char('t')) => {
                return self.handle_input(InputEvent::TestLatency);
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
        assert_eq!(state.title(), "Configs");

        state.handle(key(KeyCode::Char(']'))).unwrap();
        assert_eq!(state.title(), "Status");

        state.handle(key(KeyCode::Char('['))).unwrap();
        assert_eq!(state.title(), "Configs");
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
}
