//! Local IPC transport: socket path resolution, listeners, and connectors.

use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::ApiError;

/// A boxed, object-safe duplex byte stream.
pub trait AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncStream for T {}

/// A boxed duplex stream used for one client connection.
pub type BoxStream = Box<dyn AsyncStream>;

/// Where the kernel listens for clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketPath {
    /// The OS-specific address (a named-pipe name or a filesystem path).
    pub path: String,
}

impl SocketPath {
    /// The platform default address.
    ///
    /// An explicit `FERGENTS_SOCKET` environment variable overrides everything
    /// (used by tests and by deployments running more than one kernel).
    ///
    /// - Windows: `\\.\pipe\fergentsd`
    /// - Unix: `$XDG_RUNTIME_DIR/fergentsd.sock`, falling back to
    ///   `<data_dir>/fergentsd.sock`.
    pub fn default_for(data_dir: &std::path::Path) -> Self {
        if let Ok(path) = std::env::var("FERGENTS_SOCKET") {
            if !path.is_empty() {
                return SocketPath { path };
            }
        }
        #[cfg(windows)]
        {
            let _ = data_dir;
            SocketPath {
                path: r"\\.\pipe\fergentsd".to_string(),
            }
        }
        #[cfg(unix)]
        {
            if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
                if !runtime.is_empty() {
                    return SocketPath {
                        path: format!("{runtime}/fergentsd.sock"),
                    };
                }
            }
            SocketPath {
                path: data_dir
                    .join("fergentsd.sock")
                    .to_string_lossy()
                    .into_owned(),
            }
        }
    }

    /// Build a socket address from an explicit path (used by tests).
    pub fn from_path(path: impl Into<String>) -> Self {
        SocketPath { path: path.into() }
    }
}

/// A listener accepting client connections.
pub enum IpcListener {
    #[cfg(unix)]
    Unix(UnixListener),
    #[cfg(windows)]
    Windows(WindowsListener),
}

impl IpcListener {
    /// Bind the listener at `socket`, replacing a stale Unix socket file.
    pub fn bind(socket: &SocketPath) -> Result<Self, ApiError> {
        #[cfg(unix)]
        {
            Ok(IpcListener::Unix(UnixListener::bind(socket)?))
        }
        #[cfg(windows)]
        {
            Ok(IpcListener::Windows(WindowsListener::bind(socket)?))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = socket;
            Err(ApiError::Io(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported platform",
            )))
        }
    }

    /// Accept the next client connection.
    pub async fn accept(&mut self) -> Result<BoxStream, ApiError> {
        match self {
            #[cfg(unix)]
            IpcListener::Unix(listener) => listener.accept().await,
            #[cfg(windows)]
            IpcListener::Windows(listener) => listener.accept().await,
        }
    }
}

/// Connect to a running kernel.
pub async fn connect_stream(socket: &SocketPath) -> Result<BoxStream, ApiError> {
    #[cfg(unix)]
    {
        let stream = tokio::net::UnixStream::connect(&socket.path).await?;
        Ok(Box::new(stream))
    }
    #[cfg(windows)]
    {
        tokio::task::spawn_blocking({
            let path = socket.path.clone();
            move || -> Result<BoxStream, ApiError> {
                let client = tokio::net::windows::named_pipe::ClientOptions::new().open(&path)?;
                Ok(Box::new(client))
            }
        })
        .await
        .map_err(|e| ApiError::Io(std::io::Error::other(e)))?
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = socket;
        Err(ApiError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "unsupported platform",
        )))
    }
}

#[cfg(unix)]
mod platform {
    use std::path::Path;

    use tokio::net::UnixListener as TokioUnixListener;

    use super::{ApiError, BoxStream, SocketPath};

    /// A Unix domain socket listener.
    pub struct UnixListener {
        inner: TokioUnixListener,
        path: String,
    }

    impl UnixListener {
        /// Bind, removing any stale socket file first.
        pub fn bind(socket: &SocketPath) -> Result<Self, ApiError> {
            let path = Path::new(&socket.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            if path.exists() {
                // A stale socket file from an unclean shutdown.
                std::fs::remove_file(path)?;
            }
            let inner = TokioUnixListener::bind(path)?;
            Ok(Self {
                inner,
                path: socket.path.clone(),
            })
        }

        /// Accept the next connection.
        pub async fn accept(&self) -> Result<BoxStream, ApiError> {
            let (stream, _addr) = self.inner.accept().await?;
            Ok(Box::new(stream) as BoxStream)
        }
    }

    impl Drop for UnixListener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
use platform::UnixListener;

#[cfg(windows)]
mod platform {
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    use super::{ApiError, BoxStream, SocketPath};

    /// A Windows named-pipe listener.
    ///
    /// Named pipes accept one client per instance, so the listener pre-creates
    /// the next instance after handing off the connected one.
    pub struct WindowsListener {
        path: String,
        pending: Option<NamedPipeServer>,
    }

    impl WindowsListener {
        /// Create the first pipe instance.
        ///
        /// `first_pipe_instance(true)` fails if a pipe already exists, which
        /// enforces single-instance semantics.
        pub fn bind(socket: &SocketPath) -> Result<Self, ApiError> {
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&socket.path)?;
            Ok(Self {
                path: socket.path.clone(),
                pending: Some(server),
            })
        }

        fn create_next(&self) -> Result<NamedPipeServer, ApiError> {
            // Retry briefly: creating a new instance can transiently fail with
            // ERROR_PIPE_BUSY until the previous instance is released.
            let mut last_err = None;
            for _ in 0..10 {
                match ServerOptions::new().create(&self.path) {
                    Ok(server) => return Ok(server),
                    Err(err) => {
                        last_err = Some(err);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                }
            }
            Err(ApiError::Io(last_err.unwrap_or_else(|| {
                std::io::Error::other("failed to create named pipe instance")
            })))
        }

        /// Wait for a client, then arm the next instance.
        pub async fn accept(&mut self) -> Result<BoxStream, ApiError> {
            let server = self.pending.take().ok_or_else(|| {
                ApiError::Io(std::io::Error::other("listener has no pending pipe"))
            })?;
            server.connect().await?;
            self.pending = Some(self.create_next()?);
            Ok(Box::new(server) as BoxStream)
        }
    }
}

#[cfg(windows)]
use platform::WindowsListener;
