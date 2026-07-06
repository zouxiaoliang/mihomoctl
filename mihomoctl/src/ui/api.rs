use std::{fmt::Debug, time::Duration};

use mihomoctl_core::{
    model::{Delay, Proxies},
    serde_json::{to_string_pretty, Value},
    Clash, LongHaul, Result,
};
use crossterm::event::KeyCode;
use serde::Serialize;
use tui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Widget},
};

use crate::{
    interactive::ControllerKind,
    ui::{
        components::{MovableListItem, MovableListManage, MovableListState},
        utils::get_text_style,
    },
    Action, ListEvent,
};

pub type ApiListState<'a> =
    MovableListState<'a, ApiItem, crate::interactive::Noop>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiOperation {
    Version,
    Logs,
    LogsWs,
    Traffic,
    TrafficWs,
    Memory,
    MemoryWs,
    FlushFakeIpCache,
    FlushDnsCache,
    GetConfigs,
    ReloadConfigs,
    PatchConfigs,
    UpdateGeo,
    Restart,
    Upgrade,
    UpgradeUi,
    UpgradeGeo,
    GetGroups,
    GetGroup,
    GetGroupDelay,
    GetProxies,
    GetProxy,
    SelectProxy,
    ClearProxyFixed,
    GetProxyDelay,
    GetProxyProviders,
    GetProxyProvider,
    UpdateProxyProvider,
    HealthcheckProxyProvider,
    GetProxyProviderProxy,
    HealthcheckProxyProviderProxy,
    GetRules,
    DisableRules,
    GetRuleProviders,
    UpdateRuleProvider,
    GetConnections,
    ConnectionsWs,
    CloseConnections,
    CloseConnection,
    DnsQuery,
    GetStorage,
    PutStorage,
    DeleteStorage,
    DebugGc,
    DebugPprof,
    DebugPprofHeap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiCategory {
    Runtime,
    Cache,
    Config,
    Proxy,
    Rule,
    Connection,
    Dns,
    Storage,
    Debug,
}

impl ApiCategory {
    fn label(self) -> &'static str {
        use ApiCategory::*;
        match self {
            Runtime => "runtime",
            Cache => "cache",
            Config => "config",
            Proxy => "proxy",
            Rule => "rule",
            Connection => "conn",
            Dns => "dns",
            Storage => "storage",
            Debug => "debug",
        }
    }

    fn color(self) -> Color {
        use ApiCategory::*;
        match self {
            Runtime => Color::Cyan,
            Cache => Color::Yellow,
            Config => Color::Magenta,
            Proxy => Color::Green,
            Rule => Color::LightYellow,
            Connection => Color::LightBlue,
            Dns => Color::Blue,
            Storage => Color::Gray,
            Debug => Color::Red,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiParams {
    pub log_level: String,
    pub log_format: String,
    pub connection_interval: String,
    pub group_name: String,
    pub proxy_name: String,
    pub proxy_provider: String,
    pub provider_proxy: String,
    pub rule_provider: String,
    pub connection_id: String,
    pub rules_disable_json: String,
    pub config_path: String,
    pub config_force: String,
    pub config_patch_json: String,
    pub payload_path: String,
    pub payload: String,
    pub upgrade_channel: String,
    pub upgrade_force: String,
    pub dns_name: String,
    pub dns_type: String,
    pub storage_key: String,
    pub storage_value_json: String,
    pub expected: String,
    pub pprof_profile: String,
    pub pprof_raw: String,
}

impl Default for ApiParams {
    fn default() -> Self {
        Self {
            log_level: String::new(),
            log_format: String::new(),
            connection_interval: String::new(),
            group_name: String::new(),
            proxy_name: String::new(),
            proxy_provider: String::new(),
            provider_proxy: String::new(),
            rule_provider: String::new(),
            connection_id: String::new(),
            rules_disable_json: r#"{"0":true}"#.to_owned(),
            config_path: String::new(),
            config_force: "false".to_owned(),
            config_patch_json: "{}".to_owned(),
            payload_path: String::new(),
            payload: String::new(),
            upgrade_channel: String::new(),
            upgrade_force: "false".to_owned(),
            dns_name: "example.com".to_owned(),
            dns_type: "A".to_owned(),
            storage_key: "mihomoctl".to_owned(),
            storage_value_json: r#"{"source":"mihomoctl"}"#.to_owned(),
            expected: "200-299".to_owned(),
            pprof_profile: "heap".to_owned(),
            pprof_raw: "true".to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiParamField {
    LogLevel,
    LogFormat,
    ConnectionInterval,
    GroupName,
    ProxyName,
    ProxyProvider,
    ProviderProxy,
    RuleProvider,
    ConnectionId,
    RulesDisableJson,
    ConfigPath,
    ConfigForce,
    ConfigPatchJson,
    PayloadPath,
    Payload,
    UpgradeChannel,
    UpgradeForce,
    DnsName,
    DnsType,
    StorageKey,
    StorageValueJson,
    Expected,
    PprofProfile,
    PprofRaw,
}

impl ApiParamField {
    fn label(self) -> &'static str {
        use ApiParamField::*;
        match self {
            LogLevel => "log level",
            LogFormat => "log format",
            ConnectionInterval => "connection interval",
            GroupName => "group",
            ProxyName => "proxy",
            ProxyProvider => "proxy provider",
            ProviderProxy => "provider proxy",
            RuleProvider => "rule provider",
            ConnectionId => "connection id",
            RulesDisableJson => "rules disable",
            ConfigPath => "config path",
            ConfigForce => "config force",
            ConfigPatchJson => "config patch",
            PayloadPath => "payload path",
            Payload => "payload",
            UpgradeChannel => "upgrade channel",
            UpgradeForce => "upgrade force",
            DnsName => "dns name",
            DnsType => "dns type",
            StorageKey => "storage key",
            StorageValueJson => "storage value",
            Expected => "expected status",
            PprofProfile => "pprof profile",
            PprofRaw => "pprof raw",
        }
    }

    fn get<'a>(self, params: &'a ApiParams) -> &'a str {
        use ApiParamField::*;
        match self {
            LogLevel => &params.log_level,
            LogFormat => &params.log_format,
            ConnectionInterval => &params.connection_interval,
            GroupName => &params.group_name,
            ProxyName => &params.proxy_name,
            ProxyProvider => &params.proxy_provider,
            ProviderProxy => &params.provider_proxy,
            RuleProvider => &params.rule_provider,
            ConnectionId => &params.connection_id,
            RulesDisableJson => &params.rules_disable_json,
            ConfigPath => &params.config_path,
            ConfigForce => &params.config_force,
            ConfigPatchJson => &params.config_patch_json,
            PayloadPath => &params.payload_path,
            Payload => &params.payload,
            UpgradeChannel => &params.upgrade_channel,
            UpgradeForce => &params.upgrade_force,
            DnsName => &params.dns_name,
            DnsType => &params.dns_type,
            StorageKey => &params.storage_key,
            StorageValueJson => &params.storage_value_json,
            Expected => &params.expected,
            PprofProfile => &params.pprof_profile,
            PprofRaw => &params.pprof_raw,
        }
    }

    fn get_mut<'a>(self, params: &'a mut ApiParams) -> &'a mut String {
        use ApiParamField::*;
        match self {
            LogLevel => &mut params.log_level,
            LogFormat => &mut params.log_format,
            ConnectionInterval => &mut params.connection_interval,
            GroupName => &mut params.group_name,
            ProxyName => &mut params.proxy_name,
            ProxyProvider => &mut params.proxy_provider,
            ProviderProxy => &mut params.provider_proxy,
            RuleProvider => &mut params.rule_provider,
            ConnectionId => &mut params.connection_id,
            RulesDisableJson => &mut params.rules_disable_json,
            ConfigPath => &mut params.config_path,
            ConfigForce => &mut params.config_force,
            ConfigPatchJson => &mut params.config_patch_json,
            PayloadPath => &mut params.payload_path,
            Payload => &mut params.payload,
            UpgradeChannel => &mut params.upgrade_channel,
            UpgradeForce => &mut params.upgrade_force,
            DnsName => &mut params.dns_name,
            DnsType => &mut params.dns_type,
            StorageKey => &mut params.storage_key,
            StorageValueJson => &mut params.storage_value_json,
            Expected => &mut params.expected,
            PprofProfile => &mut params.pprof_profile,
            PprofRaw => &mut params.pprof_raw,
        }
    }
}

impl ApiOperation {
    pub const ALL: &'static [Self] = &[
        Self::Version,
        Self::Logs,
        Self::LogsWs,
        Self::Traffic,
        Self::TrafficWs,
        Self::Memory,
        Self::MemoryWs,
        Self::FlushFakeIpCache,
        Self::FlushDnsCache,
        Self::GetConfigs,
        Self::ReloadConfigs,
        Self::PatchConfigs,
        Self::UpdateGeo,
        Self::Restart,
        Self::Upgrade,
        Self::UpgradeUi,
        Self::UpgradeGeo,
        Self::GetGroups,
        Self::GetGroup,
        Self::GetGroupDelay,
        Self::GetProxies,
        Self::GetProxy,
        Self::SelectProxy,
        Self::ClearProxyFixed,
        Self::GetProxyDelay,
        Self::GetProxyProviders,
        Self::GetProxyProvider,
        Self::UpdateProxyProvider,
        Self::HealthcheckProxyProvider,
        Self::GetProxyProviderProxy,
        Self::HealthcheckProxyProviderProxy,
        Self::GetRules,
        Self::DisableRules,
        Self::GetRuleProviders,
        Self::UpdateRuleProvider,
        Self::GetConnections,
        Self::ConnectionsWs,
        Self::CloseConnections,
        Self::CloseConnection,
        Self::DnsQuery,
        Self::GetStorage,
        Self::PutStorage,
        Self::DeleteStorage,
        Self::DebugGc,
        Self::DebugPprof,
        Self::DebugPprofHeap,
    ];

    pub fn supports(self, kind: ControllerKind) -> bool {
        match kind {
            ControllerKind::Mihomo => true,
            ControllerKind::Clash => {
                use ApiOperation::*;
                matches!(
                    self,
                    Logs
                        | Traffic
                        | Version
                        | GetConfigs
                        | ReloadConfigs
                        | PatchConfigs
                        | GetProxies
                        | GetProxy
                        | SelectProxy
                        | GetProxyDelay
                        | GetRules
                        | GetConnections
                        | CloseConnections
                        | CloseConnection
                        | GetProxyProviders
                        | GetProxyProvider
                        | UpdateProxyProvider
                        | HealthcheckProxyProvider
                        | DnsQuery
                )
            }
        }
    }

    pub fn method(self) -> &'static str {
        use ApiOperation::*;
        match self {
            Logs | Traffic | Memory | Version | GetConfigs | GetGroups | GetGroup
            | GetGroupDelay | GetProxies | GetProxy | GetProxyDelay | GetProxyProviders
            | GetProxyProvider | HealthcheckProxyProvider | GetProxyProviderProxy
            | HealthcheckProxyProviderProxy | GetRules | GetRuleProviders | GetConnections
            | DnsQuery | GetStorage | DebugPprof | DebugPprofHeap => "GET",
            LogsWs | TrafficWs | MemoryWs | ConnectionsWs => "WS",
            FlushFakeIpCache | FlushDnsCache | UpdateGeo | Restart | Upgrade | UpgradeUi
            | UpgradeGeo => "POST",
            ReloadConfigs | SelectProxy | UpdateProxyProvider | UpdateRuleProvider
            | PutStorage | DebugGc => "PUT",
            PatchConfigs | DisableRules => "PATCH",
            ClearProxyFixed | CloseConnections | CloseConnection | DeleteStorage => "DELETE",
        }
    }

    fn category(self) -> ApiCategory {
        use ApiCategory::*;
        use ApiOperation::*;
        match self {
            Logs | LogsWs | Traffic | TrafficWs | Memory | MemoryWs | Version => Runtime,
            FlushFakeIpCache | FlushDnsCache => Cache,
            GetConfigs | ReloadConfigs | PatchConfigs | UpdateGeo | Restart | Upgrade
            | UpgradeUi | UpgradeGeo => Config,
            GetGroups | GetGroup | GetGroupDelay | GetProxies | GetProxy | SelectProxy
            | ClearProxyFixed | GetProxyDelay | GetProxyProviders | GetProxyProvider
            | UpdateProxyProvider | HealthcheckProxyProvider | GetProxyProviderProxy
            | HealthcheckProxyProviderProxy => Proxy,
            GetRules | DisableRules | GetRuleProviders | UpdateRuleProvider => Rule,
            GetConnections | ConnectionsWs | CloseConnections | CloseConnection => Connection,
            DnsQuery => Dns,
            GetStorage | PutStorage | DeleteStorage => Storage,
            DebugGc | DebugPprof | DebugPprofHeap => Debug,
        }
    }

    pub fn path(self) -> &'static str {
        use ApiOperation::*;
        match self {
            Logs => "/logs",
            LogsWs => "/logs",
            Traffic => "/traffic",
            TrafficWs => "/traffic",
            Memory => "/memory",
            MemoryWs => "/memory",
            Version => "/version",
            FlushFakeIpCache => "/cache/fakeip/flush",
            FlushDnsCache => "/cache/dns/flush",
            GetConfigs | ReloadConfigs | PatchConfigs => "/configs",
            UpdateGeo => "/configs/geo",
            Restart => "/restart",
            Upgrade => "/upgrade",
            UpgradeUi => "/upgrade/ui",
            UpgradeGeo => "/upgrade/geo",
            GetGroups => "/group",
            GetGroup => "/group/GLOBAL",
            GetGroupDelay => "/group/GLOBAL/delay",
            GetProxies => "/proxies",
            GetProxy => "/proxies/DIRECT",
            SelectProxy | ClearProxyFixed => "/proxies/GLOBAL",
            GetProxyDelay => "/proxies/DIRECT/delay",
            GetProxyProviders => "/providers/proxies",
            GetProxyProvider | UpdateProxyProvider => "/providers/proxies/default",
            HealthcheckProxyProvider => "/providers/proxies/default/healthcheck",
            GetProxyProviderProxy => "/providers/proxies/default/DIRECT",
            HealthcheckProxyProviderProxy => {
                "/providers/proxies/default/DIRECT/healthcheck"
            }
            GetRules => "/rules",
            DisableRules => "/rules/disable",
            GetRuleProviders => "/providers/rules",
            UpdateRuleProvider => "/providers/rules/default",
            GetConnections | ConnectionsWs | CloseConnections => "/connections",
            CloseConnection => "/connections/:id",
            DnsQuery => "/dns/query",
            GetStorage | PutStorage | DeleteStorage => "/storage/mihomoctl",
            DebugGc => "/debug/gc",
            DebugPprof => "/debug/pprof",
            DebugPprofHeap => "/debug/pprof/heap?raw=true",
        }
    }

    pub fn title(self) -> &'static str {
        use ApiOperation::*;
        match self {
            Logs => "stream logs",
            LogsWs => "websocket logs",
            Traffic => "stream traffic",
            TrafficWs => "websocket traffic",
            Memory => "stream memory",
            MemoryWs => "websocket memory",
            Version => "get version",
            FlushFakeIpCache => "flush fake-ip cache",
            FlushDnsCache => "flush dns cache",
            GetConfigs => "get configs",
            ReloadConfigs => "reload configs",
            PatchConfigs => "patch configs",
            UpdateGeo => "update geo via configs",
            Restart => "restart core",
            Upgrade => "upgrade core",
            UpgradeUi => "upgrade ui",
            UpgradeGeo => "upgrade geo",
            GetGroups => "get groups",
            GetGroup => "get group",
            GetGroupDelay => "test group",
            GetProxies => "get proxies",
            GetProxy => "get proxy",
            SelectProxy => "select proxy in group",
            ClearProxyFixed => "clear group fixed",
            GetProxyDelay => "test DIRECT proxy",
            GetProxyProviders => "get proxy providers",
            GetProxyProvider => "get proxy provider",
            UpdateProxyProvider => "update proxy provider",
            HealthcheckProxyProvider => "healthcheck provider",
            GetProxyProviderProxy => "get provider proxy",
            HealthcheckProxyProviderProxy => "healthcheck provider proxy",
            GetRules => "get rules",
            DisableRules => "disable rules by index",
            GetRuleProviders => "get rule providers",
            UpdateRuleProvider => "update rule provider",
            GetConnections => "get connections",
            ConnectionsWs => "websocket connections",
            CloseConnections => "close all connections",
            CloseConnection => "close connection by id",
            DnsQuery => "query example.com A",
            GetStorage => "get storage mihomoctl",
            PutStorage => "put storage mihomoctl",
            DeleteStorage => "delete storage mihomoctl",
            DebugGc => "run debug gc",
            DebugPprof => "get pprof index",
            DebugPprofHeap => "get pprof profile",
        }
    }

    pub fn preview_path(self, params: &ApiParams) -> String {
        use ApiOperation::*;
        match self {
            ReloadConfigs => {
                if parse_bool(&params.config_force) {
                    "/configs?force=true".to_owned()
                } else {
                    "/configs".to_owned()
                }
            }
            Upgrade => {
                let mut query = Vec::new();
                if !params.upgrade_channel.is_empty() {
                    query.push(format!("channel={}", params.upgrade_channel));
                }
                if parse_bool(&params.upgrade_force) {
                    query.push("force=true".to_owned());
                }
                if query.is_empty() {
                    "/upgrade".to_owned()
                } else {
                    format!("/upgrade?{}", query.join("&"))
                }
            }
            Logs | LogsWs => prefixed(&log_endpoint(params)),
            GetConnections | ConnectionsWs => prefixed(&connections_endpoint(params)),
            GetGroup => format!("/group/{}", name_or_placeholder(&params.group_name, "selected-group")),
            GetGroupDelay => delay_preview_path(
                "group",
                name_or_placeholder(&params.group_name, "selected-group"),
                params,
            ),
            GetProxy => format!("/proxies/{}", name_or_placeholder(&params.proxy_name, "selected-proxy")),
            SelectProxy | ClearProxyFixed => {
                format!("/proxies/{}", name_or_placeholder(&params.group_name, "selected-group"))
            }
            GetProxyDelay => delay_preview_path(
                "proxies",
                name_or_placeholder(&params.proxy_name, "selected-proxy"),
                params,
            ),
            GetProxyProvider | UpdateProxyProvider => {
                format!("/providers/proxies/{}", name_or_placeholder(&params.proxy_provider, "provider"))
            }
            HealthcheckProxyProvider => format!(
                "/providers/proxies/{}/healthcheck",
                name_or_placeholder(&params.proxy_provider, "provider")
            ),
            GetProxyProviderProxy => format!(
                "/providers/proxies/{}/{}",
                name_or_placeholder(&params.proxy_provider, "provider"),
                name_or_placeholder(&params.provider_proxy, "proxy")
            ),
            HealthcheckProxyProviderProxy => format!(
                "/providers/proxies/{}/{}/healthcheck?url=<test-url>&timeout=<timeout>",
                name_or_placeholder(&params.proxy_provider, "provider"),
                name_or_placeholder(&params.provider_proxy, "proxy")
            ),
            UpdateRuleProvider => format!(
                "/providers/rules/{}",
                name_or_placeholder(&params.rule_provider, "provider")
            ),
            CloseConnection => format!(
                "/connections/{}",
                name_or_placeholder(&params.connection_id, "first-active-id")
            ),
            DnsQuery => format!(
                "/dns/query?name={}&type={}",
                params.dns_name, params.dns_type
            ),
            GetStorage | PutStorage | DeleteStorage => format!("/storage/{}", params.storage_key),
            DebugPprofHeap => prefixed(&pprof_profile_endpoint(params)),
            _ => self.path().to_owned(),
        }
    }

    pub fn requires_confirmation(self) -> bool {
        use ApiOperation::*;
        matches!(
            self,
            ReloadConfigs | UpdateGeo | Restart | Upgrade | UpgradeUi | UpgradeGeo
                | ClearProxyFixed
                | UpdateProxyProvider
                | UpdateRuleProvider
                | CloseConnections
                | CloseConnection
                | DeleteStorage
        )
    }

    fn param_fields(self) -> &'static [ApiParamField] {
        use ApiOperation::*;
        use ApiParamField::*;
        match self {
            Logs | LogsWs => &[LogLevel, LogFormat],
            GetConnections | ConnectionsWs => &[ConnectionInterval],
            ReloadConfigs => &[ConfigPath, ConfigForce],
            PatchConfigs => &[ConfigPatchJson],
            Restart => &[PayloadPath, Payload],
            Upgrade => &[UpgradeChannel, UpgradeForce],
            GetGroup => &[GroupName],
            GetGroupDelay => &[GroupName, Expected],
            GetProxy => &[ProxyName],
            SelectProxy => &[GroupName, ProxyName],
            ClearProxyFixed => &[GroupName],
            GetProxyDelay => &[ProxyName, Expected],
            GetProxyProvider | UpdateProxyProvider | HealthcheckProxyProvider => &[ProxyProvider],
            GetProxyProviderProxy | HealthcheckProxyProviderProxy => {
                &[ProxyProvider, ProviderProxy]
            }
            DisableRules => &[RulesDisableJson],
            UpdateRuleProvider => &[RuleProvider],
            CloseConnection => &[ConnectionId],
            DnsQuery => &[DnsName, DnsType],
            GetStorage | DeleteStorage => &[StorageKey],
            PutStorage => &[StorageKey, StorageValueJson],
            DebugPprofHeap => &[PprofProfile, PprofRaw],
            _ => &[],
        }
    }

    pub fn invoke_for_kind(
        self,
        params: &ApiParams,
        clash: &Clash,
        kind: ControllerKind,
        test_url: &str,
        timeout: u64,
    ) -> String {
        if !self.supports(kind) {
            return format!("operation requires mihomo controller: {}", self.title());
        }

        self.invoke(params, clash, test_url, timeout)
    }

    pub fn invoke(self, params: &ApiParams, clash: &Clash, test_url: &str, timeout: u64) -> String {
        use ApiOperation::*;
        match self {
            Logs => stream_result(clash.get_log_with_options(
                opt_str(&params.log_level),
                log_format_structured(params),
            )),
            LogsWs => websocket_next_result(clash, &log_endpoint(params)),
            Traffic => stream_result(clash.get_traffic()),
            TrafficWs => websocket_next_result(clash, "traffic"),
            Memory => stream_result(clash.get_memory()),
            MemoryWs => websocket_next_result(clash, "memory"),
            Version => debug_result(clash.get_version()),
            FlushFakeIpCache => unit_result(clash.flush_fakeip_cache()),
            FlushDnsCache => unit_result(clash.flush_dns_cache()),
            GetConfigs => debug_result(clash.get_configs()),
            ReloadConfigs => unit_result(clash.reload_configs(
                parse_bool(&params.config_force),
                params.config_path.as_str(),
            )),
            PatchConfigs => match parse_json(&params.config_patch_json) {
                Ok(value) => json_result(clash.patch_configs(value)),
                Err(err) => err,
            },
            UpdateGeo => unit_result(clash.update_geo(
                opt_str(&params.payload_path),
                opt_str(&params.payload),
            )),
            Restart => unit_result(clash.restart(
                opt_str(&params.payload_path),
                opt_str(&params.payload),
            )),
            Upgrade => unit_result(clash.upgrade(
                opt_str(&params.upgrade_channel),
                parse_bool(&params.upgrade_force),
                opt_str(&params.payload_path),
                opt_str(&params.payload),
            )),
            UpgradeUi => unit_result(clash.upgrade_ui()),
            UpgradeGeo => unit_result(clash.upgrade_geo(
                opt_str(&params.payload_path),
                opt_str(&params.payload),
            )),
            GetGroups => json_result(clash.get_groups()),
            GetGroup => with_group_param(params, clash, |group| json_result(clash.get_group(&group))),
            GetGroupDelay => with_group_param(params, clash, |group| {
                delay_result(clash.get_group_delay(
                    &group,
                    test_url,
                    timeout,
                    opt_str(&params.expected),
                ))
            }),
            GetProxies => debug_result(clash.get_proxies()),
            GetProxy => with_proxy_param(params, clash, |proxy| debug_result(clash.get_proxy(&proxy))),
            SelectProxy => with_selector_member_param(params, clash, |group, proxy| {
                unit_result(clash.set_proxygroup_selected(&group, &proxy))
            }),
            GetProxyDelay => with_proxy_param(params, clash, |proxy| {
                delay_result(clash.get_proxy_delay_expected(
                    &proxy,
                    test_url,
                    timeout,
                    opt_str(&params.expected),
                ))
            }),
            ClearProxyFixed => {
                with_group_param(params, clash, |group| unit_result(clash.clear_proxy_fixed(&group)))
            }
            GetProxyProviders => json_result(clash.get_proxy_providers()),
            GetProxyProvider => with_proxy_provider_param(params, clash, |provider| {
                json_result(clash.get_proxy_provider(&provider))
            }),
            UpdateProxyProvider => with_proxy_provider_param(params, clash, |provider| {
                unit_result(clash.update_proxy_provider(&provider))
            }),
            HealthcheckProxyProvider => with_proxy_provider_param(params, clash, |provider| {
                json_result(clash.healthcheck_proxy_provider(&provider))
            }),
            GetProxyProviderProxy => with_proxy_provider_proxy_param(params, clash, |provider, proxy| {
                json_result(clash.get_proxy_provider_proxy(&provider, &proxy))
            }),
            HealthcheckProxyProviderProxy => {
                with_proxy_provider_proxy_param(params, clash, |provider, proxy| {
                    delay_result(clash.healthcheck_proxy_provider_proxy(
                        &provider, &proxy, test_url, timeout,
                    ))
                })
            }
            GetRules => debug_result(clash.get_rules()),
            DisableRules => match parse_rules_disable(&params.rules_disable_json) {
                Ok(rules) => unit_result(clash.disable_rules(rules)),
                Err(err) => err,
            },
            GetRuleProviders => json_result(clash.get_rule_providers()),
            UpdateRuleProvider => with_rule_provider_param(params, clash, |provider| {
                unit_result(clash.update_rule_provider(&provider))
            }),
            GetConnections => match parse_optional_u64(&params.connection_interval, "interval") {
                Ok(interval) => debug_result(clash.get_connections_with_interval(interval)),
                Err(err) => err,
            },
            ConnectionsWs => websocket_next_result(clash, &connections_endpoint(params)),
            CloseConnections => unit_result(clash.close_connections()),
            CloseConnection => match opt_str(&params.connection_id) {
                Some(id) => unit_result(clash.close_one_connection(id)),
                None => close_first_connection(clash),
            },
            DnsQuery => json_result(clash.dns_query(&params.dns_name, &params.dns_type)),
            GetStorage => json_result(clash.get_storage(&params.storage_key)),
            PutStorage => match parse_json(&params.storage_value_json) {
                Ok(value) => unit_result(clash.put_storage(&params.storage_key, value)),
                Err(err) => err,
            },
            DeleteStorage => unit_result(clash.delete_storage(&params.storage_key)),
            DebugGc => unit_result(clash.debug_gc()),
            DebugPprof => text_result(clash.debug_pprof()),
            DebugPprofHeap => text_result(
                clash.debug_pprof_profile(
                    opt_str(&params.pprof_profile).unwrap_or("heap"),
                    parse_bool(&params.pprof_raw),
                ),
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiItem {
    pub operation: ApiOperation,
    pub last_result: Option<String>,
    params: ApiParams,
    param_index: usize,
    editing: bool,
    confirm: bool,
}

impl ApiItem {
    fn new(operation: ApiOperation) -> Self {
        Self {
            operation,
            last_result: None,
            params: ApiParams::default(),
            param_index: 0,
            editing: false,
            confirm: false,
        }
    }

    pub fn catalog() -> Vec<Self> {
        Self::catalog_for(ApiOperation::ALL)
    }

    pub fn catalog_for_kind(kind: ControllerKind) -> Vec<Self> {
        let operations = ApiOperation::ALL
            .iter()
            .copied()
            .filter(|operation| operation.supports(kind))
            .collect::<Vec<_>>();
        Self::catalog_for(&operations)
    }

    pub fn catalog_for(operations: &[ApiOperation]) -> Vec<Self> {
        operations
            .iter()
            .map(|operation| Self {
                ..Self::new(*operation)
            })
            .collect()
    }
}

impl<'a> MovableListItem<'a> for ApiItem {
    fn to_spans(&self) -> Spans<'a> {
        let category = self.operation.category();
        let method_style = match self.operation.method() {
            "GET" => Style::default().fg(Color::Green),
            "WS" => Style::default().fg(Color::Cyan),
            "POST" => Style::default().fg(Color::Yellow),
            "PUT" | "PATCH" => Style::default().fg(Color::Blue),
            "DELETE" => Style::default().fg(Color::Red),
            _ => Style::default(),
        }
        .add_modifier(Modifier::BOLD);

        let param = self
            .current_param()
            .map(|field| {
                let marker = if self.editing { "*" } else { "" };
                format!(" [{}{}={}]", marker, field.label(), field.get(&self.params))
            })
            .unwrap_or_default();

        let result = self
            .last_result
            .as_deref()
            .unwrap_or("press Enter to invoke");

        Spans(vec![
            Span::styled(
                format!("{:<8}", category.label()),
                Style::default()
                    .fg(category.color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<6}", self.operation.method()), method_style),
            Span::raw(format!("{:<58}", self.operation.preview_path(&self.params))),
            Span::styled(
                format!("{:<32}", self.operation.title()),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(param, Style::default().fg(Color::Blue)),
            Span::raw(" "),
            Span::raw(result.to_owned()),
        ])
    }
}

impl ApiItem {
    fn param_fields(&self) -> &'static [ApiParamField] {
        self.operation.param_fields()
    }

    fn current_param(&self) -> Option<ApiParamField> {
        let fields = self.param_fields();
        if fields.is_empty() {
            None
        } else {
            Some(fields[self.param_index.min(fields.len() - 1)])
        }
    }
}

#[derive(Clone, Debug)]
pub struct ApiMenu<'a> {
    title: String,
    state: &'a ApiListState<'a>,
}

impl<'a> ApiMenu<'a> {
    pub fn new<TITLE: Into<String>>(title: TITLE, state: &'a ApiListState<'a>) -> Self {
        Self {
            title: title.into(),
            state,
        }
    }
}

impl<'a> Widget for ApiMenu<'a> {
    fn render(self, area: Rect, buf: &mut tui::buffer::Buffer) {
        let block = Block::default()
            .title(self.title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));
        let inner = block.inner(area);
        let height = inner.height as usize;
        let selected = self
            .state
            .current_item_index()
            .unwrap_or_default()
            .min(self.state.len().saturating_sub(1));
        let start = selected.saturating_sub(height.saturating_sub(1));

        let items = self
            .state
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
            .map(|(index, item)| {
                let selected = index == selected;
                let marker = if selected { ">" } else { " " };
                let style = if selected {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::REVERSED)
                } else {
                    get_text_style()
                };
                let param = item
                    .current_param()
                    .map(|field| {
                        let edit = if item.editing { "*" } else { "" };
                        format!("{}{}={}", edit, field.label(), field.get(&item.params))
                    })
                    .unwrap_or_else(|| "-".to_owned());
                let status = item
                    .last_result
                    .as_deref()
                    .map(compact_result)
                    .unwrap_or_else(|| {
                        if item.current_param().is_some() && item.operation.requires_confirmation() {
                            "i edit, Enter twice".to_owned()
                        } else if item.current_param().is_some() {
                            "i edit, Enter run".to_owned()
                        } else if item.operation.requires_confirmation() {
                            "Enter twice to confirm".to_owned()
                        } else {
                            "Enter to run".to_owned()
                        }
                    });

                ListItem::new(Spans::from(vec![
                    Span::styled(format!("{marker} "), style),
                    Span::styled(format!("{:<28}", item.operation.title()), style),
                    Span::styled(format!(" {:<34}", param), style.fg(Color::Blue)),
                    Span::styled(status, style.fg(Color::DarkGray)),
                ]))
            })
            .collect::<Vec<_>>();

        block.render(area, buf);
        List::new(items).render(inner, buf);
    }
}

fn compact_result(result: &str) -> String {
    const LIMIT: usize = 72;
    let result = result.replace('\n', " ");
    if result.chars().count() > LIMIT {
        result.chars().take(LIMIT).collect::<String>() + "..."
    } else {
        result
    }
}

pub fn default_api_state<'a>() -> ApiListState<'a> {
    api_state_for(ApiItem::catalog())
}

pub fn api_state_for_kind<'a>(kind: ControllerKind) -> ApiListState<'a> {
    api_state_for(ApiItem::catalog_for_kind(kind))
}

pub fn config_core_api_state<'a>() -> ApiListState<'a> {
    config_core_api_state_for_kind(ControllerKind::Mihomo)
}

pub fn config_core_api_state_for_kind<'a>(kind: ControllerKind) -> ApiListState<'a> {
    use ApiOperation::*;
    let operations = [
        GetConfigs,
        ReloadConfigs,
        PatchConfigs,
        UpdateGeo,
        Restart,
        Upgrade,
        UpgradeUi,
        UpgradeGeo,
    ]
    .into_iter()
    .filter(|operation| operation.supports(kind))
    .collect::<Vec<_>>();
    api_state_for(ApiItem::catalog_for(&operations))
}

pub fn dns_api_state<'a>() -> ApiListState<'a> {
    dns_api_state_for_kind(ControllerKind::Mihomo)
}

pub fn dns_api_state_for_kind<'a>(kind: ControllerKind) -> ApiListState<'a> {
    use ApiOperation::*;
    let operations = [FlushFakeIpCache, FlushDnsCache, DnsQuery]
        .into_iter()
        .filter(|operation| operation.supports(kind))
        .collect::<Vec<_>>();
    api_state_for(ApiItem::catalog_for(&operations))
}

fn api_state_for<'a>(items: Vec<ApiItem>) -> ApiListState<'a> {
    let mut state = ApiListState::new(items);
    state.with_index();
    state.normal_order();
    state.header(Spans::from(Span::styled(
        format!(
            "{:<8}{:<6}{:<58}{:<32} {}",
            "GROUP", "METHOD", "PATH", "TITLE", "PARAM / RESULT"
        ),
        Style::default().fg(Color::DarkGray),
    )));
    state
}

pub fn contains_operation(operation: ApiOperation) -> bool {
    ApiOperation::ALL.contains(&operation)
}

pub fn current_operation(state: &ApiListState) -> Option<ApiOperation> {
    state
        .current_item_index()
        .and_then(|index| state.get(index))
        .map(|item| item.operation)
}

pub fn current_result<'a>(state: &'a ApiListState) -> Option<&'a str> {
    state
        .current_item_index()
        .and_then(|index| state.get(index))
        .and_then(|item| item.last_result.as_deref())
}

pub fn current_needs_input(state: &ApiListState) -> bool {
    state
        .current_item_index()
        .and_then(|index| state.get(index))
        .map(|item| item.current_param().is_some() && !item.confirm)
        .unwrap_or(false)
}

pub fn current_param_label(state: &ApiListState) -> Option<&'static str> {
    state
        .current_item_index()
        .and_then(|index| state.get(index))
        .and_then(|item| item.current_param())
        .map(ApiParamField::label)
}

pub fn current_param_value<'a>(state: &'a ApiListState) -> Option<&'a str> {
    let item = state
        .current_item_index()
        .and_then(|index| state.get(index))?;
    let field = item.current_param()?;
    Some(field.get(&item.params))
}

pub fn select_operation(state: &mut ApiListState, operation: ApiOperation) -> Option<()> {
    let pos = state
        .iter()
        .position(|item| item.operation == operation)?;
    state.end();
    for _ in 0..pos {
        state.handle(ListEvent {
            fast: false,
            code: KeyCode::Down,
        });
    }
    Some(())
}

pub fn submit_current(state: &mut ApiListState) -> Option<Action> {
    let pos = state.current_item_index()?;
    let item = state.get_mut(pos)?;

    if item.operation.requires_confirmation() && !item.confirm {
        item.confirm = true;
        item.last_result = Some(format!(
            "dangerous operation: press Enter again to confirm {} {}",
            item.operation.method(),
            item.operation.path()
        ));
        return None;
    }

    let operation = item.operation;
    let params = item.params.clone();
    clear_confirmations(state);
    Some(Action::InvokeApi { operation, params })
}

pub fn cancel_confirmation(state: &mut ApiListState) {
    clear_confirmations(state);
}

pub fn update_result(state: &mut ApiListState, operation: ApiOperation, result: String) {
    if let Some(item) = state
        .iter_mut()
        .find(|item| item.operation == operation)
    {
        item.confirm = false;
        item.last_result = Some(result);
    }
}

fn clear_confirmations(state: &mut ApiListState) {
    for item in state.iter_mut() {
        item.confirm = false;
    }
}

pub fn begin_edit(state: &mut ApiListState) -> bool {
    let Some(pos) = state.current_item_index() else {
        return false;
    };
    let Some(item) = state.get_mut(pos) else {
        return false;
    };
    if item.current_param().is_none() {
        return false;
    }
    item.editing = true;
    true
}

pub fn end_edit(state: &mut ApiListState) {
    if let Some(item) = state
        .current_item_index()
        .and_then(|index| state.get_mut(index))
    {
        item.editing = false;
    }
}

pub fn is_editing(state: &ApiListState) -> bool {
    state
        .current_item_index()
        .and_then(|index| state.get(index))
        .map(|item| item.editing)
        .unwrap_or(false)
}

pub fn next_param(state: &mut ApiListState) -> bool {
    let Some(pos) = state.current_item_index() else {
        return false;
    };
    let Some(item) = state.get_mut(pos) else {
        return false;
    };
    let len = item.param_fields().len();
    if len == 0 {
        return false;
    }
    item.param_index = (item.param_index + 1) % len;
    true
}

pub fn input_char(state: &mut ApiListState, ch: char) -> bool {
    let Some(pos) = state.current_item_index() else {
        return false;
    };
    let Some(item) = state.get_mut(pos) else {
        return false;
    };
    if !item.editing {
        return false;
    }
    let Some(field) = item.current_param() else {
        return false;
    };
    field.get_mut(&mut item.params).push(ch);
    true
}

pub fn backspace_current_param(state: &mut ApiListState) -> bool {
    let Some(pos) = state.current_item_index() else {
        return false;
    };
    let Some(item) = state.get_mut(pos) else {
        return false;
    };
    if !item.editing {
        return false;
    }
    let Some(field) = item.current_param() else {
        return false;
    };
    field.get_mut(&mut item.params).pop().is_some()
}

pub fn clear_current_param(state: &mut ApiListState) -> bool {
    let Some(pos) = state.current_item_index() else {
        return false;
    };
    let Some(item) = state.get_mut(pos) else {
        return false;
    };
    let Some(field) = item.current_param() else {
        return false;
    };
    field.get_mut(&mut item.params).clear();
    true
}

pub fn set_current_param_value(state: &mut ApiListState, value: &str) -> bool {
    let Some(pos) = state.current_item_index() else {
        return false;
    };
    let Some(item) = state.get_mut(pos) else {
        return false;
    };
    let Some(field) = item.current_param() else {
        return false;
    };
    *field.get_mut(&mut item.params) = value.to_owned();
    true
}

fn prefixed(endpoint: &str) -> String {
    format!("/{endpoint}")
}

fn encoded(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn name_or_placeholder(value: &str, placeholder: &str) -> String {
    match opt_str(value) {
        Some(value) => value.to_owned(),
        None => format!("<{placeholder}>"),
    }
}

fn query_endpoint(base: &str, query: Vec<(&str, String)>) -> String {
    if query.is_empty() {
        return base.to_owned();
    }

    let query = query
        .into_iter()
        .map(|(key, value)| format!("{key}={}", encoded(&value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}

fn log_format_structured(params: &ApiParams) -> bool {
    params.log_format.eq_ignore_ascii_case("structured") || parse_bool(&params.log_format)
}

fn log_endpoint(params: &ApiParams) -> String {
    let mut query = Vec::new();
    if let Some(level) = opt_str(&params.log_level) {
        query.push(("level", level.to_owned()));
    }
    if log_format_structured(params) {
        query.push(("format", "structured".to_owned()));
    }
    query_endpoint("logs", query)
}

fn connections_endpoint(params: &ApiParams) -> String {
    let mut query = Vec::new();
    if let Some(interval) = opt_str(&params.connection_interval) {
        query.push(("interval", interval.to_owned()));
    }
    query_endpoint("connections", query)
}

fn pprof_profile_endpoint(params: &ApiParams) -> String {
    let profile = opt_str(&params.pprof_profile).unwrap_or("heap");
    if parse_bool(&params.pprof_raw) {
        format!("debug/pprof/{profile}?raw=true")
    } else {
        format!("debug/pprof/{profile}")
    }
}

fn delay_preview_path(prefix: &str, name: String, params: &ApiParams) -> String {
    let mut path = format!("/{prefix}/{name}/delay?url=<test-url>&timeout=<timeout>");
    if let Some(expected) = opt_str(&params.expected) {
        path.push_str("&expected=");
        path.push_str(expected);
    }
    path
}

fn unit_result(result: Result<()>) -> String {
    match result {
        Ok(()) => "OK".to_owned(),
        Err(err) => format!("ERR {}", err),
    }
}

fn text_result(result: Result<String>) -> String {
    match result {
        Ok(value) => pretty_json_text(&value).unwrap_or(value),
        Err(err) => format!("ERR {}", err),
    }
}

fn delay_result(result: Result<Delay>) -> String {
    match result {
        Ok(value) => format!("OK {} ms", value.delay),
        Err(err) => format!("ERR {}", err),
    }
}

fn json_result(result: Result<Value>) -> String {
    match result {
        Ok(value) => to_string_pretty(&value).unwrap_or_else(|_| value.to_string()),
        Err(err) => format!("ERR {}", err),
    }
}

fn debug_result<T: Debug + Serialize>(result: Result<T>) -> String {
    match result {
        Ok(value) => to_string_pretty(&value).unwrap_or_else(|_| truncate(format!("{:?}", value), 160)),
        Err(err) => format!("ERR {}", err),
    }
}

fn stream_result<T: serde::de::DeserializeOwned>(result: Result<LongHaul<T>>) -> String {
    match result {
        Ok(_) => "OK connected to stream".to_owned(),
        Err(err) => format!("ERR {}", err),
    }
}

fn pretty_json_text(raw: &str) -> Option<String> {
    mihomoctl_core::serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|value| to_string_pretty(&value).ok())
}

fn parse_json(raw: &str) -> std::result::Result<Value, String> {
    mihomoctl_core::serde_json::from_str(raw)
        .map_err(|err| format!("ERR invalid json: {}", err))
}

fn parse_optional_u64(raw: &str, label: &str) -> std::result::Result<Option<u64>, String> {
    match opt_str(raw.trim()) {
        Some(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|err| format!("ERR invalid {label}: {err}")),
        None => Ok(None),
    }
}

fn parse_rules_disable(raw: &str) -> std::result::Result<Vec<(usize, bool)>, String> {
    let value = parse_json(raw)?;
    let object = value
        .as_object()
        .ok_or_else(|| "ERR rules disable json must be an object".to_owned())?;
    object
        .iter()
        .map(|(index, disabled)| {
            let index = index
                .parse::<usize>()
                .map_err(|err| format!("ERR invalid rule index {index}: {err}"))?;
            let disabled = disabled
                .as_bool()
                .ok_or_else(|| format!("ERR rule {index} value must be boolean"))?;
            Ok((index, disabled))
        })
        .collect()
}

fn opt_str(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
}

fn websocket_next_result(clash: &Clash, endpoint: &str) -> String {
    match clash.websocket_next_raw_with_timeout(endpoint, Duration::from_secs(2)) {
        Ok(value) => match pretty_json_text(&value) {
            Some(json) => format!("WS\n{json}"),
            None => truncate(format!("WS {}", value), 160),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn close_first_connection(clash: &Clash) -> String {
    match clash.get_connections() {
        Ok(connections) => match connections.connections.first() {
            Some(connection) => unit_result(clash.close_one_connection(&connection.id)),
            None => "ERR no active connection".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn configured(value: &str) -> Option<String> {
    opt_str(value).map(str::to_owned)
}

fn with_group_param<F>(params: &ApiParams, clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match configured(&params.group_name) {
        Some(group) => call(group),
        None => with_group(clash, call),
    }
}

fn with_proxy_param<F>(params: &ApiParams, clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match configured(&params.proxy_name) {
        Some(proxy) => call(proxy),
        None => with_proxy(clash, call),
    }
}

fn with_selector_member_param<F>(params: &ApiParams, clash: &Clash, call: F) -> String
where
    F: FnOnce(String, String) -> String,
{
    match (
        configured(&params.group_name),
        configured(&params.proxy_name),
    ) {
        (Some(group), Some(proxy)) => call(group, proxy),
        (Some(_), None) => "ERR proxy name is required with a manual group".to_owned(),
        (None, Some(_)) => "ERR group name is required with a manual proxy".to_owned(),
        (None, None) => with_selector_member(clash, call),
    }
}

fn with_proxy_provider_param<F>(params: &ApiParams, clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match configured(&params.proxy_provider) {
        Some(provider) => call(provider),
        None => with_proxy_provider(clash, call),
    }
}

fn with_proxy_provider_proxy_param<F>(params: &ApiParams, clash: &Clash, call: F) -> String
where
    F: FnOnce(String, String) -> String,
{
    match (
        configured(&params.proxy_provider),
        configured(&params.provider_proxy),
    ) {
        (Some(provider), Some(proxy)) => call(provider, proxy),
        (Some(provider), None) => match clash.get_proxy_provider(&provider) {
            Ok(value) => match first_named_child(&value, "proxies") {
                Some(proxy) => call(provider, proxy),
                None => "ERR no proxy provider proxy found".to_owned(),
            },
            Err(err) => format!("ERR {}", err),
        },
        (None, Some(proxy)) => {
            with_proxy_provider(clash, |provider| call(provider, proxy))
        }
        (None, None) => with_proxy_provider_proxy(clash, call),
    }
}

fn with_rule_provider_param<F>(params: &ApiParams, clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match configured(&params.rule_provider) {
        Some(provider) => call(provider),
        None => with_rule_provider(clash, call),
    }
}

fn with_group<F>(clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match clash.get_proxies() {
        Ok(proxies) => match first_group_name(&proxies) {
            Some(group) => call(group),
            None => "ERR no proxy group found".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn with_proxy<F>(clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match clash.get_proxies() {
        Ok(proxies) => match first_proxy_name(&proxies) {
            Some(proxy) => call(proxy),
            None => "ERR no proxy found".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn with_selector_member<F>(clash: &Clash, call: F) -> String
where
    F: FnOnce(String, String) -> String,
{
    match clash.get_proxies() {
        Ok(proxies) => match first_selector_member(&proxies) {
            Some((group, proxy)) => call(group, proxy),
            None => "ERR no selector group member found".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn first_group_name(proxies: &Proxies) -> Option<String> {
    proxies.groups().map(|(name, _)| name.to_owned()).next()
}

fn first_proxy_name(proxies: &Proxies) -> Option<String> {
    proxies
        .normal()
        .chain(proxies.built_ins())
        .map(|(name, _)| name.to_owned())
        .next()
        .or_else(|| proxies.keys().next().cloned())
}

fn first_selector_member(proxies: &Proxies) -> Option<(String, String)> {
    proxies
        .selectors()
        .find_map(|(group, proxy)| {
            proxy
                .now
                .clone()
                .or_else(|| proxy.all.as_ref().and_then(|all| all.first().cloned()))
                .map(|member| (group.to_owned(), member))
        })
}

fn with_proxy_provider<F>(clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match clash.get_proxy_providers() {
        Ok(providers) => match first_named_child(&providers, "providers") {
            Some(provider) => call(provider),
            None => "ERR no proxy provider found".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn with_proxy_provider_proxy<F>(clash: &Clash, call: F) -> String
where
    F: FnOnce(String, String) -> String,
{
    with_proxy_provider(clash, |provider| match clash.get_proxy_provider(&provider) {
        Ok(value) => match first_named_child(&value, "proxies") {
            Some(proxy) => call(provider, proxy),
            None => "ERR no proxy provider proxy found".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    })
}

fn with_rule_provider<F>(clash: &Clash, call: F) -> String
where
    F: FnOnce(String) -> String,
{
    match clash.get_rule_providers() {
        Ok(providers) => match first_named_child(&providers, "providers") {
            Some(provider) => call(provider),
            None => "ERR no rule provider found".to_owned(),
        },
        Err(err) => format!("ERR {}", err),
    }
}

fn first_named_child(value: &Value, container: &str) -> Option<String> {
    let inner = value.get(container).unwrap_or(value);

    if let Some(object) = inner.as_object() {
        return object.keys().next().cloned();
    }

    inner.as_array().and_then(|items| {
        items.iter().find_map(|item| {
            item.get("name")
                .and_then(|name| name.as_str())
                .map(str::to_owned)
        })
    })
}

fn truncate(mut value: String, limit: usize) -> String {
    if value.len() > limit {
        value.truncate(limit.saturating_sub(3));
        value.push_str("...");
    }
    value
}

#[cfg(test)]
mod tests {
    use mihomoctl_core::serde_json::json;
    use tui::{buffer::Buffer, layout::Rect};

    use super::*;

    fn rendered(buf: &Buffer, area: Rect) -> String {
        let mut ret = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                ret.push_str(buf.get(x, y).symbol.as_str());
            }
        }
        ret
    }

    #[test]
    fn first_named_child_reads_provider_response_shapes() {
        assert_eq!(
            first_named_child(&json!({"providers": {"sub": {}}}), "providers"),
            Some("sub".to_owned())
        );
        assert_eq!(
            first_named_child(&json!({"sub": {}}), "providers"),
            Some("sub".to_owned())
        );
        assert_eq!(
            first_named_child(
                &json!({"proxies": [{"name": "Hong Kong"}, {"name": "Japan"}]}),
                "proxies",
            ),
            Some("Hong Kong".to_owned())
        );
    }

    #[test]
    fn proxy_choice_helpers_read_current_proxy_model() {
        use std::collections::HashMap;

        use mihomoctl_core::model::{Proxies, Proxy, ProxyType};

        let proxies = Proxies {
            proxies: HashMap::from([
                (
                    "Auto".to_owned(),
                    Proxy {
                        proxy_type: ProxyType::Selector,
                        history: vec![],
                        udp: None,
                        all: Some(vec!["DIRECT".to_owned()]),
                        now: Some("DIRECT".to_owned()),
                    },
                ),
                (
                    "DIRECT".to_owned(),
                    Proxy {
                        proxy_type: ProxyType::Direct,
                        history: vec![],
                        udp: None,
                        all: None,
                        now: None,
                    },
                ),
            ]),
        };

        assert_eq!(first_group_name(&proxies), Some("Auto".to_owned()));
        assert_eq!(first_proxy_name(&proxies), Some("DIRECT".to_owned()));
        assert_eq!(
            first_selector_member(&proxies),
            Some(("Auto".to_owned(), "DIRECT".to_owned()))
        );
    }

    #[test]
    fn api_catalog_contains_websocket_variants() {
        assert!(contains_operation(ApiOperation::LogsWs));
        assert!(contains_operation(ApiOperation::TrafficWs));
        assert!(contains_operation(ApiOperation::MemoryWs));
        assert!(contains_operation(ApiOperation::ConnectionsWs));
    }

    #[test]
    fn clash_api_catalog_excludes_mihomo_only_operations() {
        let clash = api_state_for_kind(ControllerKind::Clash)
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();

        for operation in [
            ApiOperation::Memory,
            ApiOperation::MemoryWs,
            ApiOperation::FlushFakeIpCache,
            ApiOperation::GetGroups,
            ApiOperation::Restart,
            ApiOperation::DisableRules,
            ApiOperation::GetStorage,
            ApiOperation::DebugPprof,
            ApiOperation::ConnectionsWs,
        ] {
            assert!(!clash.contains(&operation), "{operation:?} should be hidden");
        }

        assert!(clash.contains(&ApiOperation::Version));
        assert!(clash.contains(&ApiOperation::GetProxies));
        assert!(clash.contains(&ApiOperation::DnsQuery));
    }

    #[test]
    fn mihomo_api_catalog_keeps_full_catalog() {
        let mihomo = api_state_for_kind(ControllerKind::Mihomo)
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();
        let full = default_api_state()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();

        assert_eq!(mihomo, full);
    }

    #[test]
    fn unsupported_mihomo_operation_is_guarded_before_request() {
        let clash = Clash::builder("http://127.0.0.1:1").unwrap().build();
        let result = ApiOperation::Memory.invoke_for_kind(
            &ApiParams::default(),
            &clash,
            ControllerKind::Clash,
            "http://example.com",
            2000,
        );

        assert!(result.contains("requires mihomo controller"));
    }

    #[test]
    fn api_catalog_covers_current_metacubex_reference() {
        let catalog = ApiOperation::ALL
            .iter()
            .map(|operation| (operation.method(), operation.path()))
            .collect::<std::collections::BTreeSet<_>>();

        for expected in [
            ("GET", "/logs"),
            ("WS", "/logs"),
            ("GET", "/traffic"),
            ("WS", "/traffic"),
            ("GET", "/memory"),
            ("WS", "/memory"),
            ("GET", "/version"),
            ("POST", "/cache/fakeip/flush"),
            ("POST", "/cache/dns/flush"),
            ("GET", "/configs"),
            ("PUT", "/configs"),
            ("PATCH", "/configs"),
            ("POST", "/configs/geo"),
            ("POST", "/restart"),
            ("POST", "/upgrade"),
            ("POST", "/upgrade/ui"),
            ("POST", "/upgrade/geo"),
            ("GET", "/group"),
            ("GET", "/group/GLOBAL"),
            ("GET", "/group/GLOBAL/delay"),
            ("GET", "/proxies"),
            ("GET", "/proxies/DIRECT"),
            ("PUT", "/proxies/GLOBAL"),
            ("DELETE", "/proxies/GLOBAL"),
            ("GET", "/proxies/DIRECT/delay"),
            ("GET", "/providers/proxies"),
            ("GET", "/providers/proxies/default"),
            ("PUT", "/providers/proxies/default"),
            ("GET", "/providers/proxies/default/healthcheck"),
            ("GET", "/providers/proxies/default/DIRECT"),
            ("GET", "/providers/proxies/default/DIRECT/healthcheck"),
            ("GET", "/rules"),
            ("PATCH", "/rules/disable"),
            ("GET", "/providers/rules"),
            ("PUT", "/providers/rules/default"),
            ("GET", "/connections"),
            ("WS", "/connections"),
            ("DELETE", "/connections"),
            ("DELETE", "/connections/:id"),
            ("GET", "/dns/query"),
            ("GET", "/storage/mihomoctl"),
            ("PUT", "/storage/mihomoctl"),
            ("DELETE", "/storage/mihomoctl"),
            ("PUT", "/debug/gc"),
            ("GET", "/debug/pprof"),
            ("GET", "/debug/pprof/heap?raw=true"),
        ] {
            assert!(catalog.contains(&expected), "{expected:?} is missing from API catalog");
        }
    }

    #[test]
    fn dns_quick_page_hides_storage_and_debug_endpoints() {
        let dns = dns_api_state()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();

        for operation in [
            ApiOperation::GetStorage,
            ApiOperation::PutStorage,
            ApiOperation::DeleteStorage,
            ApiOperation::DebugGc,
            ApiOperation::DebugPprof,
            ApiOperation::DebugPprofHeap,
        ] {
            assert!(!dns.contains(&operation), "{operation:?} should be hidden");
        }

        assert!(dns.contains(&ApiOperation::FlushFakeIpCache));
        assert!(dns.contains(&ApiOperation::FlushDnsCache));
        assert!(dns.contains(&ApiOperation::DnsQuery));

        let full = default_api_state()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>();
        assert!(full.contains(&ApiOperation::GetStorage));
        assert!(full.contains(&ApiOperation::DebugPprofHeap));
    }

    #[test]
    fn api_items_render_control_categories() {
        let restart = ApiItem::catalog()
            .into_iter()
            .find(|item| item.operation == ApiOperation::Restart)
            .unwrap();
        let rendered = restart
            .to_spans()
            .0
            .into_iter()
            .map(|span| span.content.into_owned())
            .collect::<String>();

        assert!(rendered.contains("config"));
        assert!(rendered.contains("POST"));
        assert!(rendered.contains("/restart"));
        assert!(rendered.contains("restart core"));
    }

    #[test]
    fn api_menu_renders_functions_not_endpoint_table() {
        let state = config_core_api_state();
        let area = Rect::new(0, 0, 90, 8);
        let mut buf = Buffer::empty(area);

        ApiMenu::new("Core", &state).render(area, &mut buf);
        let rendered = rendered(&buf, area);

        assert!(rendered.contains("get configs"));
        assert!(rendered.contains("reload configs"));
        assert!(rendered.contains("Enter to run"));
        assert!(!rendered.contains("METHOD"));
        assert!(!rendered.contains("/configs"));
    }

    #[test]
    fn api_results_keep_full_pretty_payload_for_popup() {
        let long_value = "x".repeat(240);
        let json = json_result(Ok(json!({
            "outer": {
                "value": long_value
            }
        })));
        assert!(json.contains('\n'));
        assert!(json.contains(&"x".repeat(200)));

        let text = text_result(Ok("alpha\nbeta".to_owned()));
        assert_eq!(text, "alpha\nbeta");
    }

    #[test]
    fn typed_api_results_are_pretty_json() {
        use mihomoctl_core::model::{Version, VersionPayload};

        let version = Version {
            premium: Some(true),
            version: VersionPayload::Raw("1.2.3".to_owned()),
        };

        let rendered = debug_result(Ok(version));

        assert!(rendered.contains('\n'));
        assert!(rendered.contains("\"premium\": true"));
        assert!(rendered.contains("\"version\": \"1.2.3\""));
        assert!(!rendered.contains("Version {"));
    }

    #[test]
    fn edited_dns_parameter_is_sent_with_action() {
        let mut state = default_api_state();
        select_operation(&mut state, ApiOperation::DnsQuery).unwrap();

        begin_edit(&mut state);
        clear_current_param(&mut state);
        for ch in "openai.com".chars() {
            input_char(&mut state, ch);
        }

        let action = submit_current(&mut state).unwrap();
        assert_eq!(
            action,
            Action::InvokeApi {
                operation: ApiOperation::DnsQuery,
                params: ApiParams {
                    dns_name: "openai.com".to_owned(),
                    ..ApiParams::default()
                }
            }
        );
    }

    #[test]
    fn geo_upgrade_operations_do_not_prompt_for_payload() {
        for operation in [ApiOperation::UpdateGeo, ApiOperation::UpgradeGeo] {
            let mut state = default_api_state();
            select_operation(&mut state, operation).unwrap();

            assert!(!current_needs_input(&state));
        }
    }

    #[test]
    fn current_api_parameter_can_be_filled_by_picker() {
        let mut state = default_api_state();
        select_operation(&mut state, ApiOperation::GetGroup).unwrap();

        assert!(set_current_param_value(&mut state, "Auto"));

        let action = submit_current(&mut state).unwrap();
        assert_eq!(
            action,
            Action::InvokeApi {
                operation: ApiOperation::GetGroup,
                params: ApiParams {
                    group_name: "Auto".to_owned(),
                    ..ApiParams::default()
                }
            }
        );
    }

    #[test]
    fn preview_path_uses_editable_parameters() {
        let params = ApiParams {
            log_level: "debug".to_owned(),
            log_format: "structured".to_owned(),
            connection_interval: "750".to_owned(),
            group_name: "Auto".to_owned(),
            proxy_name: "Proxy 1".to_owned(),
            proxy_provider: "sub".to_owned(),
            provider_proxy: "Provider Proxy".to_owned(),
            rule_provider: "geosite".to_owned(),
            connection_id: "abc123".to_owned(),
            rules_disable_json: r#"{"0":true}"#.to_owned(),
            dns_name: "openai.com".to_owned(),
            dns_type: "AAAA".to_owned(),
            storage_key: "dash".to_owned(),
            pprof_profile: "allocs".to_owned(),
            pprof_raw: "false".to_owned(),
            ..ApiParams::default()
        };

        assert_eq!(
            ApiOperation::Logs.preview_path(&params),
            "/logs?level=debug&format=structured"
        );
        assert_eq!(
            ApiOperation::LogsWs.preview_path(&params),
            "/logs?level=debug&format=structured"
        );
        assert_eq!(
            ApiOperation::ConnectionsWs.preview_path(&params),
            "/connections?interval=750"
        );
        assert_eq!(
            ApiOperation::GetGroup.preview_path(&params),
            "/group/Auto"
        );
        assert_eq!(
            ApiOperation::GetProxy.preview_path(&params),
            "/proxies/Proxy 1"
        );
        assert_eq!(
            ApiOperation::GetProxyProviderProxy.preview_path(&params),
            "/providers/proxies/sub/Provider Proxy"
        );
        assert_eq!(
            ApiOperation::UpdateRuleProvider.preview_path(&params),
            "/providers/rules/geosite"
        );
        assert_eq!(
            ApiOperation::CloseConnection.preview_path(&params),
            "/connections/abc123"
        );
        assert_eq!(
            ApiOperation::DnsQuery.preview_path(&params),
            "/dns/query?name=openai.com&type=AAAA"
        );
        assert_eq!(
            ApiOperation::GetStorage.preview_path(&params),
            "/storage/dash"
        );
        assert_eq!(
            ApiOperation::DebugPprofHeap.preview_path(&params),
            "/debug/pprof/allocs"
        );
    }
}
