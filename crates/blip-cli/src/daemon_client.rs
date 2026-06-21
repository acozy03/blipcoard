use blip_api::{DaemonCommand, DaemonRequest, DaemonResponse};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

pub struct DaemonClient {}

impl DaemonClient {
    pub fn unavailable() -> Self {
        Self {}
    }

    pub fn request(&self, request: DaemonRequest) -> Result<DaemonResponse, DaemonClientError> {
        Err(DaemonClientError::Unavailable {
            command: request.command,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaemonClientError {
    Unavailable { command: DaemonCommand },
}

impl Display for DaemonClientError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable { command } => write!(
                formatter,
                "daemon API transport is not available for `{}` yet; this command still needs a direct store fallback",
                command.as_str()
            ),
        }
    }
}

impl Error for DaemonClientError {}

#[cfg(test)]
mod tests {
    use super::*;
    use blip_api::{DAEMON_API_VERSION, DaemonRequestPayload};

    #[test]
    fn unavailable_client_reports_command_name() {
        let client = DaemonClient::unavailable();
        let request = DaemonRequest {
            api_version: DAEMON_API_VERSION,
            request_id: "cli-health".to_owned(),
            command: DaemonCommand::Health,
            payload: DaemonRequestPayload::Health,
        };

        let error = client
            .request(request)
            .expect_err("transport should be unavailable in the foundation client");

        assert_eq!(
            error.to_string(),
            "daemon API transport is not available for `health` yet; this command still needs a direct store fallback"
        );
    }
}
