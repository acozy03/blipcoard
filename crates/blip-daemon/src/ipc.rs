use blip_api::{DaemonRequest, DaemonResponse};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum IpcError {
    UnsupportedPlatform,
    Io(std::io::Error),
    Serialization(serde_json::Error),
    EmptyRequest,
}

impl Display for IpcError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(
                formatter,
                "daemon IPC is only available on Unix platforms in this release"
            ),
            Self::Io(error) => write!(formatter, "daemon IPC io error: {error}"),
            Self::Serialization(error) => write!(formatter, "daemon IPC JSON error: {error}"),
            Self::EmptyRequest => write!(formatter, "daemon IPC client closed without a request"),
        }
    }
}

impl Error for IpcError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::UnsupportedPlatform | Self::EmptyRequest => None,
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

pub struct DaemonIpcServer {
    socket_path: PathBuf,
}

impl DaemonIpcServer {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_owned(),
        }
    }

    pub fn serve<H>(&self, handler: H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        platform::serve(&self.socket_path, handler)
    }

    pub fn serve_one<H>(&self, handler: H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        platform::serve_one(&self.socket_path, handler)
    }
}

#[cfg(unix)]
mod platform {
    use super::IpcError;
    use blip_api::{DaemonRequest, DaemonResponse};
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;

    pub fn serve<H>(socket_path: &Path, handler: H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        let listener = bind(socket_path)?;

        for stream in listener.incoming() {
            handle_stream(stream?, &handler)?;
        }

        Ok(())
    }

    pub fn serve_one<H>(socket_path: &Path, handler: H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        let listener = bind(socket_path)?;
        let (stream, _) = listener.accept()?;
        handle_stream(stream, &handler)
    }

    fn bind(socket_path: &Path) -> Result<UnixListener, IpcError> {
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent)?;
        }

        match fs::remove_file(socket_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(IpcError::Io(error)),
        }

        Ok(UnixListener::bind(socket_path)?)
    }

    fn handle_stream<H>(stream: UnixStream, handler: &H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        let mut request_line = String::new();
        let mut reader = BufReader::new(stream);
        let bytes_read = reader.read_line(&mut request_line)?;
        if bytes_read == 0 {
            return Err(IpcError::EmptyRequest);
        }

        let request = serde_json::from_str::<DaemonRequest>(request_line.trim_end())?;
        let response = handler(request);

        let stream = reader.get_mut();
        serde_json::to_writer(&mut *stream, &response)?;
        stream.write_all(b"\n")?;
        stream.flush()?;

        Ok(())
    }
}

#[cfg(not(unix))]
mod platform {
    use super::IpcError;
    use blip_api::{DaemonRequest, DaemonResponse};
    use std::path::Path;

    pub fn serve<H>(_socket_path: &Path, _handler: H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        Err(IpcError::UnsupportedPlatform)
    }

    pub fn serve_one<H>(_socket_path: &Path, _handler: H) -> Result<(), IpcError>
    where
        H: Fn(DaemonRequest) -> DaemonResponse,
    {
        Err(IpcError::UnsupportedPlatform)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use blip_api::{
        DAEMON_API_VERSION, DaemonCommand, DaemonRequestPayload, DaemonResponsePayload,
        DaemonResponseStatus, DaemonVersionResponse,
    };
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn server_handles_one_newline_delimited_request() {
        let socket_path = unique_socket_path("blip-daemon-ipc-test");
        let server = DaemonIpcServer::new(&socket_path);
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            server
                .serve_one(|request| {
                    DaemonResponse::ok(
                        request.request_id,
                        request.command,
                        DaemonResponsePayload::Version(DaemonVersionResponse {
                            api_version: DAEMON_API_VERSION,
                            daemon_version: "0.1.0-test".to_owned(),
                        }),
                    )
                })
                .expect("server should handle one request");
            std::fs::remove_file(server_socket_path).ok();
        });

        wait_for_socket(&socket_path);
        let mut stream = UnixStream::connect(&socket_path).expect("client should connect");
        let request = DaemonRequest::new(
            "transport-1",
            DaemonCommand::Version,
            DaemonRequestPayload::Version,
        );
        serde_json::to_writer(&mut stream, &request).expect("request should serialize");
        stream.write_all(b"\n").expect("request should flush");

        let mut response_line = String::new();
        BufReader::new(stream)
            .read_line(&mut response_line)
            .expect("response line should read");
        let response =
            serde_json::from_str::<DaemonResponse>(&response_line).expect("response should decode");

        assert_eq!(response.status, DaemonResponseStatus::Ok);
        assert_eq!(
            response.payload,
            Some(DaemonResponsePayload::Version(DaemonVersionResponse {
                api_version: DAEMON_API_VERSION,
                daemon_version: "0.1.0-test".to_owned(),
            }))
        );

        handle.join().expect("server thread should join");
    }

    fn unique_socket_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}.sock", std::process::id()))
    }

    fn wait_for_socket(socket_path: &Path) {
        for _ in 0..100 {
            if socket_path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("socket was not created at {}", socket_path.display());
    }
}
