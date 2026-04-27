use anyhow::{bail, Context, Result};
use std::io::Read;
use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;

const DEFAULT_PORT: u16 = 18501;

pub struct TcpTransport {
    stream: TcpStream,
}

impl TcpTransport {
    /// Set up ADB port forwarding (run once before retrying connect).
    pub fn setup_forward(port: u16) -> Result<u16> {
        let port = if port == 0 { DEFAULT_PORT } else { port };
        let adb = find_adb();
        eprintln!("[tcp] Running: {} forward tcp:{} tcp:{}", adb, port, port);

        let output = Command::new(&adb)
            .args(["forward", &format!("tcp:{}", port), &format!("tcp:{}", port)])
            .output()
            .with_context(|| format!("Failed to run '{}'. Is ADB installed and in PATH?", adb))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("adb forward failed: {}", stderr.trim());
        }

        eprintln!("[tcp] ADB forward OK.");
        Ok(port)
    }

    /// Single non-retrying TCP connect attempt. Returns Ok(None) on connection refused/timeout.
    pub fn try_connect(port: u16) -> Result<Option<Self>> {
        let addr = format!("127.0.0.1:{}", port).parse().unwrap();
        match TcpStream::connect_timeout(&addr, Duration::from_secs(1)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_millis(200))).ok();
                Ok(Some(TcpTransport { stream }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused
                    || e.kind() == std::io::ErrorKind::TimedOut => {
                Ok(None) // phone not ready yet
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Set up ADB port forwarding and connect (single attempt, original behavior for headless).
    pub fn connect(port: u16) -> Result<Self> {
        let port = Self::setup_forward(port)?;

        eprintln!("[tcp] Connecting to 127.0.0.1:{}...", port);
        let stream = TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", port).parse().unwrap(),
            Duration::from_secs(5),
        )
        .context("Failed to connect to Android TCP server. Is the app running and capturing?")?;

        stream.set_read_timeout(Some(Duration::from_millis(200))).ok();

        eprintln!("[tcp] Connected.");
        Ok(TcpTransport { stream })
    }

    /// Read bytes from the TCP stream. Returns bytes read.
    pub fn read(&self, buf: &mut [u8], timeout_ms: u64) -> Result<usize> {
        // Update timeout if caller wants something different
        let _ = self
            .stream
            .set_read_timeout(Some(Duration::from_millis(timeout_ms.max(10))));

        let mut stream_ref = &self.stream;
        match stream_ref.read(buf) {
            Ok(0) => bail!("Connection closed by Android"),
            Ok(n) => Ok(n),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                Ok(0)
            }
            Err(e) => Err(e.into()),
        }
    }
}

/// Find adb executable — check PATH first, then common SDK locations.
fn find_adb() -> String {
    // Try PATH first
    if Command::new("adb").arg("version").output().is_ok() {
        return "adb".to_string();
    }

    // Common Windows SDK location via LOCALAPPDATA
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let candidate = std::path::PathBuf::from(local)
            .join("Android")
            .join("Sdk")
            .join("platform-tools")
            .join("adb.exe");
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    // Fall back to bare "adb" and let the error propagate
    "adb".to_string()
}
