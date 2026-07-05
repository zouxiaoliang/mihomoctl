use crate::ui::api::{ApiOperation, ApiParams};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    TestLatency { proxies: Vec<String> },
    ApplySelection { group: String, proxy: String },
    InvokeApi {
        operation: ApiOperation,
        params: ApiParams,
    },
}
