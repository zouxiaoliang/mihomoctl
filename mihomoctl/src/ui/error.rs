#[derive(Debug, thiserror::Error)]
pub enum TuiError {
    #[error("{0}")]
    InteractiveError(#[from] crate::interactive::InteractiveError),

    #[error("Mihomoctl error: {0}")]
    MihomoCtl(#[from] crate::mihomoctl::Error),

    #[error("TUI error")]
    TuiError(#[from] std::io::Error),

    #[error("TUI backend error")]
    TuiBackendErr,

    #[error("TUI interuptted error")]
    TuiInterupttedErr,

    #[error("TUI internal error")]
    TuiInternalErr,

    #[error("Set logger error ({0})")]
    SetLoggerError(#[from] log::SetLoggerError),
}

impl<T> From<std::sync::mpsc::SendError<T>> for TuiError {
    fn from(_: std::sync::mpsc::SendError<T>) -> Self {
        Self::TuiBackendErr
    }
}

pub type TuiResult<T> = std::result::Result<T, TuiError>;
