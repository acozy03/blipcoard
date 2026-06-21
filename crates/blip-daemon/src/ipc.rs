use blip_api::{DaemonRequest, DaemonResponse};
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};
use std::time::Duration;

const CLIENT_READ_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub enum IpcError {
    UnsupportedPlatform,
    AlreadyRunning { socket_path: PathBuf },
    Io(std::io::Error),
    Serialization(serde_json::Error),
    EmptyRequest,
    RequestTimeout,
}

impl Display for IpcError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => write!(
                formatter,
                "daemon IPC is only available on Unix platforms in this release"
            ),
            Self::AlreadyRunning { socket_path } => write!(
                formatter,
                "daemon IPC socket is already accepting connections at {}",
                socket_path.display()
            ),
            Self::Io(error) => write!(formatter, "daemon IPC io error: {error}"),
            Self::Serialization(error) => write!(formatter, "daemon IPC JSON error: {error}"),
            Self::EmptyRequest => write!(formatter, "daemon IPC client closed without a request"),
            Self::RequestTimeout => write!(formatter, "daemon IPC client timed out before request"),
        }
    }
}

impl Error for IpcError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
            Self::UnsupportedPlatform
            | Self::AlreadyRunning { .. }
            | Self::EmptyRequest
            | Self::RequestTimeout => None,
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
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        self.bind()?.serve(handler)
    }

    pub fn serve_one<H>(&self, handler: H) -> Result<(), IpcError>
    where
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        self.bind()?.serve_one(handler)
    }

    #[cfg(test)]
    pub(crate) fn serve_n<H>(&self, request_count: usize, handler: H) -> Result<(), IpcError>
    where
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        self.bind()?.serve_n(request_count, handler)
    }

    pub fn bind(&self) -> Result<BoundDaemonIpcServer, IpcError> {
        Ok(BoundDaemonIpcServer {
            listener: platform::bind(&self.socket_path)?,
        })
    }
}

pub struct BoundDaemonIpcServer {
    listener: platform::BoundListener,
}

impl BoundDaemonIpcServer {
    pub fn serve<H>(self, handler: H) -> Result<(), IpcError>
    where
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        self.listener.serve(handler)
    }

    pub fn serve_one<H>(self, handler: H) -> Result<(), IpcError>
    where
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        self.listener.serve_one(handler)
    }

    #[cfg(test)]
    pub(crate) fn serve_n<H>(self, request_count: usize, handler: H) -> Result<(), IpcError>
    where
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        self.listener.serve_n(request_count, handler)
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

    pub struct BoundListener {
        listener: UnixListener,
    }

    impl BoundListener {
        pub fn serve<H>(self, mut handler: H) -> Result<(), IpcError>
        where
            H: FnMut(DaemonRequest) -> DaemonResponse,
        {
            for stream in self.listener.incoming() {
                match stream {
                    Ok(stream) => {
                        if let Err(error) = handle_stream(stream, &mut handler) {
                            eprintln!("daemon IPC client request failed: {error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("daemon IPC client accept failed: {error}");
                    }
                }
            }

            Ok(())
        }

        pub fn serve_one<H>(self, mut handler: H) -> Result<(), IpcError>
        where
            H: FnMut(DaemonRequest) -> DaemonResponse,
        {
            let (stream, _) = self.listener.accept()?;
            handle_stream(stream, &mut handler)
        }

        #[cfg(test)]
        pub fn serve_n<H>(self, request_count: usize, mut handler: H) -> Result<(), IpcError>
        where
            H: FnMut(DaemonRequest) -> DaemonResponse,
        {
            for _ in 0..request_count {
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if let Err(error) = handle_stream(stream, &mut handler) {
                            eprintln!("daemon IPC client request failed: {error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("daemon IPC client accept failed: {error}");
                    }
                }
            }

            Ok(())
        }
    }

    pub(super) fn bind(socket_path: &Path) -> Result<BoundListener, IpcError> {
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent)?;
        }

        remove_stale_socket_if_present(socket_path)?;

        Ok(BoundListener {
            listener: UnixListener::bind(socket_path)?,
        })
    }

    fn remove_stale_socket_if_present(socket_path: &Path) -> Result<(), IpcError> {
        match UnixStream::connect(socket_path) {
            Ok(_) => Err(IpcError::AlreadyRunning {
                socket_path: socket_path.to_owned(),
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                remove_socket_file(socket_path)
            }
            Err(error) => Err(IpcError::Io(error)),
        }
    }

    fn remove_socket_file(socket_path: &Path) -> Result<(), IpcError> {
        match fs::remove_file(socket_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(IpcError::Io(error)),
        }
    }

    fn handle_stream<H>(stream: UnixStream, handler: &mut H) -> Result<(), IpcError>
    where
        H: FnMut(DaemonRequest) -> DaemonResponse,
    {
        stream.set_read_timeout(Some(super::CLIENT_READ_TIMEOUT))?;
        let mut request_line = String::new();
        let mut reader = BufReader::new(stream);
        let bytes_read = match reader.read_line(&mut request_line) {
            Ok(bytes_read) => bytes_read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                return Err(IpcError::RequestTimeout);
            }
            Err(error) => return Err(IpcError::Io(error)),
        };
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

    pub struct BoundListener;

    impl BoundListener {
        pub fn serve<H>(self, _handler: H) -> Result<(), IpcError>
        where
            H: FnMut(DaemonRequest) -> DaemonResponse,
        {
            Err(IpcError::UnsupportedPlatform)
        }

        pub fn serve_one<H>(self, _handler: H) -> Result<(), IpcError>
        where
            H: FnMut(DaemonRequest) -> DaemonResponse,
        {
            Err(IpcError::UnsupportedPlatform)
        }

        #[cfg(test)]
        pub fn serve_n<H>(self, _request_count: usize, _handler: H) -> Result<(), IpcError>
        where
            H: FnMut(DaemonRequest) -> DaemonResponse,
        {
            Err(IpcError::UnsupportedPlatform)
        }
    }

    pub(super) fn bind(_socket_path: &Path) -> Result<BoundListener, IpcError> {
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

    #[test]
    fn server_continues_after_empty_and_malformed_client_requests() {
        let socket_path = unique_socket_path("blip-daemon-ipc-malformed-test");
        let server = DaemonIpcServer::new(&socket_path);
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            server
                .serve_n(3, |request| {
                    DaemonResponse::ok(
                        request.request_id,
                        request.command,
                        DaemonResponsePayload::Version(DaemonVersionResponse {
                            api_version: DAEMON_API_VERSION,
                            daemon_version: "0.1.0-test".to_owned(),
                        }),
                    )
                })
                .expect("server should process bounded client attempts");
            std::fs::remove_file(server_socket_path).ok();
        });

        wait_for_socket(&socket_path);
        drop(UnixStream::connect(&socket_path).expect("empty client should connect"));

        {
            let mut stream =
                UnixStream::connect(&socket_path).expect("malformed client should connect");
            stream
                .write_all(b"not-json\n")
                .expect("malformed request should write");
        }

        let mut stream = UnixStream::connect(&socket_path).expect("valid client should connect");
        let request = DaemonRequest::new(
            "transport-after-error",
            DaemonCommand::Version,
            DaemonRequestPayload::Version,
        );
        serde_json::to_writer(&mut stream, &request).expect("request should serialize");
        stream.write_all(b"\n").expect("request should flush");

        let mut response_line = String::new();
        BufReader::new(stream)
            .read_line(&mut response_line)
            .expect("response line should read after malformed request");
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

    #[test]
    fn idle_client_does_not_block_following_valid_request() {
        let socket_path = unique_socket_path("blip-daemon-ipc-idle-client-test");
        let server = DaemonIpcServer::new(&socket_path);
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            server
                .serve_n(2, |request| {
                    DaemonResponse::ok(
                        request.request_id,
                        request.command,
                        DaemonResponsePayload::Version(DaemonVersionResponse {
                            api_version: DAEMON_API_VERSION,
                            daemon_version: "0.1.0-test".to_owned(),
                        }),
                    )
                })
                .expect("server should process bounded client attempts");
            std::fs::remove_file(server_socket_path).ok();
        });

        wait_for_socket(&socket_path);
        let idle_client = UnixStream::connect(&socket_path).expect("idle client should connect");
        let mut stream = connect_with_retry(&socket_path);
        let request = DaemonRequest::new(
            "transport-after-idle",
            DaemonCommand::Version,
            DaemonRequestPayload::Version,
        );
        serde_json::to_writer(&mut stream, &request).expect("request should serialize");
        stream.write_all(b"\n").expect("request should flush");

        let mut response_line = String::new();
        BufReader::new(stream)
            .read_line(&mut response_line)
            .expect("response line should read after idle client");
        let response =
            serde_json::from_str::<DaemonResponse>(&response_line).expect("response should decode");

        assert_eq!(response.status, DaemonResponseStatus::Ok);
        drop(idle_client);
        handle.join().expect("server thread should join");
    }

    #[test]
    fn server_does_not_bind_over_live_socket() {
        let socket_path = unique_socket_path("blip-daemon-ipc-live-socket-test");
        let server = DaemonIpcServer::new(&socket_path);
        let server_socket_path = socket_path.clone();
        let handle = thread::spawn(move || {
            server
                .serve_n(1, |request| {
                    DaemonResponse::ok(
                        request.request_id,
                        request.command,
                        DaemonResponsePayload::Version(DaemonVersionResponse {
                            api_version: DAEMON_API_VERSION,
                            daemon_version: "0.1.0-test".to_owned(),
                        }),
                    )
                })
                .expect("first server should bind and accept one probe");
            std::fs::remove_file(server_socket_path).ok();
        });

        wait_for_socket(&socket_path);
        let error = DaemonIpcServer::new(&socket_path)
            .serve_n(0, |request| {
                DaemonResponse::ok(
                    request.request_id,
                    request.command,
                    DaemonResponsePayload::Version(DaemonVersionResponse {
                        api_version: DAEMON_API_VERSION,
                        daemon_version: "should-not-run".to_owned(),
                    }),
                )
            })
            .expect_err("second server should not bind over a live socket");

        match error {
            IpcError::AlreadyRunning {
                socket_path: active_socket_path,
            } => assert_eq!(active_socket_path, socket_path),
            other => panic!("expected already-running IPC error, got {other:?}"),
        }

        handle.join().expect("server thread should join");
    }

    #[test]
    fn server_removes_stale_socket_file_before_binding() {
        let socket_path = unique_socket_path("blip-daemon-ipc-stale-socket-test");
        std::fs::write(&socket_path, b"stale socket placeholder")
            .expect("stale socket placeholder should be created");

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
                .expect("server should replace stale socket file");
            std::fs::remove_file(server_socket_path).ok();
        });

        let mut stream = connect_with_retry(&socket_path);
        let request = DaemonRequest::new(
            "transport-after-stale",
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
        handle.join().expect("server thread should join");
    }

    fn connect_with_retry(socket_path: &Path) -> UnixStream {
        let mut last_error = None;

        for _ in 0..100 {
            match UnixStream::connect(socket_path) {
                Ok(stream) => return stream,
                Err(error) => {
                    last_error = Some(error);
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }

        panic!(
            "socket was not connectable at {}: {:?}",
            socket_path.display(),
            last_error
        );
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
