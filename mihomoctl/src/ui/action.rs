use mihomoctl_core::model::Mode;

use crate::ui::api::{ApiOperation, ApiParams};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    TestLatency { proxies: Vec<String> },
    ApplySelection { group: String, proxy: String },
    SetMode { mode: Mode },
    FetchConfigs,
    ReloadConfigs,
    UpdateGeo,
    CloseConnection { id: String },
    CloseAllConnections,
    InvokeApi {
        operation: ApiOperation,
        params: ApiParams,
    },
}
