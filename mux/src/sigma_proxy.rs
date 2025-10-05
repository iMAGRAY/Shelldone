use crate::{Mux, MuxNotification, SigmaGuardData};
use anyhow::Result;
use log::{debug, warn};
use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
use promise::spawn::is_scheduler_configured;
use std::cmp::min;
use std::fmt::Write as FmtWrite;
use std::io::{self, Read, Write};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

pub struct SigmaProxyPty {
    inner: Box<dyn MasterPty>,
    reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>,
}

impl SigmaProxyPty {
    pub fn new(inner: Box<dyn MasterPty>) -> Self {
        Self::with_reporter(inner, global_reporter())
    }

    pub fn with_reporter(
        inner: Box<dyn MasterPty>,
        reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>,
    ) -> Self {
        Self { inner, reporter }
    }

    pub fn into_inner(self) -> Box<dyn MasterPty> {
        self.inner
    }
}

impl MasterPty for SigmaProxyPty {
    fn resize(&self, size: PtySize) -> Result<()> {
        self.inner.resize(size)
    }

    fn get_size(&self) -> Result<PtySize> {
        self.inner.get_size()
    }

    fn try_clone_reader(&self) -> Result<Box<dyn Read + Send>> {
        let reader = self.inner.try_clone_reader()?;
        Ok(Box::new(SigmaProxyReader::new(
            reader,
            self.reporter.clone(),
        )))
    }

    fn take_writer(&self) -> Result<Box<dyn Write + Send>> {
        let writer = self.inner.take_writer()?;
        Ok(Box::new(SigmaProxyWriter::new(
            writer,
            self.reporter.clone(),
        )))
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<libc::pid_t> {
        self.inner.process_group_leader()
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        self.inner.as_raw_fd()
    }

    #[cfg(unix)]
    fn tty_name(&self) -> Option<std::path::PathBuf> {
        self.inner.tty_name()
    }

    #[cfg(unix)]
    fn get_termios(&self) -> Option<nix::sys::termios::Termios> {
        self.inner.get_termios()
    }
}

#[derive(Debug)]
pub struct SigmaProxyChild {
    inner: Box<dyn Child>,
}

impl SigmaProxyChild {
    pub fn new(inner: Box<dyn Child>) -> Self {
        Self { inner }
    }
}

impl Child for SigmaProxyChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.inner.try_wait()
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        self.inner.wait()
    }

    fn process_id(&self) -> Option<u32> {
        self.inner.process_id()
    }

    #[cfg(windows)]
    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        self.inner.as_raw_handle()
    }
}

impl ChildKiller for SigmaProxyChild {
    fn kill(&mut self) -> io::Result<()> {
        self.inner.kill()
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        self.inner.clone_killer()
    }
}

struct SigmaProxyReader {
    inner: Box<dyn Read + Send>,
    reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>,
}

impl SigmaProxyReader {
    fn new(
        inner: Box<dyn Read + Send>,
        reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>,
    ) -> Self {
        Self { inner, reporter }
    }
}

impl Read for SigmaProxyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut tmp = vec![0u8; buf.len()];
        let n = self.inner.read(&mut tmp)?;
        if n == 0 {
            return Ok(0);
        }
        let sanitized = sanitize_output(&tmp[..n], &self.reporter);
        let len = min(sanitized.len(), buf.len());
        buf[..len].copy_from_slice(&sanitized[..len]);
        Ok(len)
    }
}

struct SigmaProxyWriter {
    inner: Box<dyn Write + Send>,
    reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>,
}

impl SigmaProxyWriter {
    fn new(
        inner: Box<dyn Write + Send>,
        reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>,
    ) -> Self {
        Self { inner, reporter }
    }
}

impl Write for SigmaProxyWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let sanitized = sanitize_input(buf, &self.reporter);
        self.inner.write(&sanitized).map(|_| buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

const ESC: u8 = 0x1B;
const BEL: u8 = 0x07;
const MAX_OSC52_PAYLOAD: usize = 8 * 1024;
const VIOLATION_PREVIEW_BYTES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigmaDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigmaViolation {
    pub direction: SigmaDirection,
    pub reason: &'static str,
    pub sequence_preview: String,
    pub sequence_len: usize,
    pub occurred_at: SystemTime,
}

pub trait SigmaPolicyReporter {
    fn report(&self, violation: SigmaViolation);
}

struct NoopReporter;

impl SigmaPolicyReporter for NoopReporter {
    fn report(&self, violation: SigmaViolation) {
        debug!(
            "sigma noop reporter {:?} {} len={} {}",
            violation.direction,
            violation.reason,
            violation.sequence_len,
            violation.sequence_preview
        );
    }
}

static REPORTER: OnceLock<RwLock<Arc<dyn SigmaPolicyReporter + Send + Sync>>> = OnceLock::new();

fn reporter_registry() -> &'static RwLock<Arc<dyn SigmaPolicyReporter + Send + Sync>> {
    REPORTER.get_or_init(|| RwLock::new(Arc::new(NoopReporter)))
}

fn global_reporter() -> Arc<dyn SigmaPolicyReporter + Send + Sync> {
    reporter_registry()
        .read()
        .expect("sigma reporter lock poisoned")
        .clone()
}

pub fn set_sigma_policy_reporter(reporter: Arc<dyn SigmaPolicyReporter + Send + Sync>) {
    *reporter_registry()
        .write()
        .expect("sigma reporter lock poisoned") = reporter;
}

fn report_violation(
    reporter: &Arc<dyn SigmaPolicyReporter + Send + Sync>,
    direction: SigmaDirection,
    sequence: &[u8],
    reason: &'static str,
) {
    let preview = preview_bytes(sequence);
    let occurred_at = SystemTime::now();
    let violation = SigmaViolation {
        direction,
        reason,
        sequence_preview: preview,
        sequence_len: sequence.len(),
        occurred_at,
    };
    reporter.report(violation.clone());
    let data = SigmaGuardData {
        direction: violation.direction,
        reason: violation.reason.to_string(),
        sequence_preview: violation.sequence_preview.clone(),
        sequence_len: violation.sequence_len,
        occurred_at: violation.occurred_at,
    };
    if is_scheduler_configured() {
        Mux::notify_from_any_thread(MuxNotification::SigmaGuard(data));
    }
}

fn preview_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::from("<empty>");
    }
    let mut buf = String::new();
    for ch in bytes.iter().take(VIOLATION_PREVIEW_BYTES) {
        let _ = write!(&mut buf, "{:02X} ", ch);
    }
    if bytes.len() > VIOLATION_PREVIEW_BYTES {
        buf.push('…');
    }
    buf.trim_end().to_string()
}

fn sanitize_input(data: &[u8], reporter: &Arc<dyn SigmaPolicyReporter + Send + Sync>) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    for chunk in data.chunks(1) {
        let byte = chunk[0];
        if (byte <= 0x08 || (0x0B..=0x0C).contains(&byte) || (0x0E..=0x1F).contains(&byte))
            && byte != ESC
        {
            warn!("Filtered control character input: 0x{byte:02x}");
            report_violation(
                reporter,
                SigmaDirection::Input,
                chunk,
                "control character filtered",
            );
            continue;
        }
        result.extend_from_slice(chunk);
    }
    result
}

fn sanitize_output(data: &[u8], reporter: &Arc<dyn SigmaPolicyReporter + Send + Sync>) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == ESC {
            match parse_escape(&data[i..]) {
                EscapeParse::Allowed(len) => {
                    result.extend_from_slice(&data[i..i + len]);
                    i += len;
                }
                EscapeParse::Filtered(len, reason) => {
                    let end = (i + len).min(data.len());
                    warn!("Filtered escape sequence: {reason}");
                    report_violation(reporter, SigmaDirection::Output, &data[i..end], reason);
                    i += len;
                }
                EscapeParse::Invalid(len) => {
                    let end = (i + len).min(data.len());
                    warn!("Invalid escape sequence length={len}");
                    report_violation(
                        reporter,
                        SigmaDirection::Output,
                        &data[i..end],
                        "invalid escape",
                    );
                    i += len;
                }
            }
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    result
}

enum EscapeParse {
    Allowed(usize),
    Filtered(usize, &'static str),
    Invalid(usize),
}

fn parse_escape(data: &[u8]) -> EscapeParse {
    if data.len() < 2 {
        return EscapeParse::Invalid(1);
    }
    match data[1] {
        b'[' => parse_csi(data),
        b']' => parse_osc(data),
        b'P' | b'^' | b'_' => parse_string(data),
        b'(' | b')' | b'*' | b'+' | b'-' | b'.' => EscapeParse::Allowed(3.min(data.len())),
        tag => {
            debug!("Unhandled escape tag: 0x{:02x}", tag);
            EscapeParse::Allowed(2)
        }
    }
}

fn parse_csi(data: &[u8]) -> EscapeParse {
    // CSI: ESC [ ... final byte 0x40-0x7E
    for (idx, &byte) in data.iter().enumerate().skip(2) {
        if (0x40..=0x7E).contains(&byte) {
            return EscapeParse::Allowed(idx + 1);
        }
    }
    EscapeParse::Invalid(data.len())
}

fn parse_osc(data: &[u8]) -> EscapeParse {
    let mut idx = 2;
    let mut code = Vec::new();
    while idx < data.len() {
        let byte = data[idx];
        if byte == b';' {
            break;
        }
        if !byte.is_ascii_digit() {
            return EscapeParse::Filtered(idx + 1, "non-numeric OSC code");
        }
        code.push(byte);
        idx += 1;
    }
    let payload_start = idx + 1;
    while idx < data.len() {
        let b = data[idx];
        if b == BEL {
            idx += 1;
            break;
        }
        if b == ESC && idx + 1 < data.len() && data[idx + 1] == b'\\' {
            idx += 2;
            break;
        }
        idx += 1;
    }
    let len = idx.min(data.len());
    let code_value = std::str::from_utf8(&code)
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    let payload = if payload_start < len {
        let terminator = if len > 0 && data[len - 1] == BEL {
            1
        } else if len > 1 && data[len - 2] == ESC && data[len - 1] == b'\\' {
            2
        } else {
            0
        };
        let end = len.saturating_sub(terminator);
        if payload_start < end {
            &data[payload_start..end]
        } else {
            &[]
        }
    } else {
        &[]
    };
    match code_value {
        Some(52) => {
            if payload.contains(&b'?') {
                return EscapeParse::Filtered(len, "OSC 52 read blocked");
            }
            if payload.len() > MAX_OSC52_PAYLOAD {
                return EscapeParse::Filtered(len, "OSC 52 payload too large");
            }
            EscapeParse::Allowed(len)
        }
        Some(0 | 2 | 4 | 8 | 133 | 1337) => EscapeParse::Allowed(len),
        Some(other) => EscapeParse::Filtered(
            len,
            match other {
                52 => "OSC 52 restricted",
                _ => "OSC code not allowed",
            },
        ),
        None => EscapeParse::Filtered(len, "invalid OSC code"),
    }
}

fn parse_string(data: &[u8]) -> EscapeParse {
    for (idx, &byte) in data.iter().enumerate().skip(2) {
        if byte == BEL {
            return EscapeParse::Allowed(idx + 1);
        }
        if byte == ESC && idx + 1 < data.len() && data[idx + 1] == b'\\' {
            return EscapeParse::Allowed(idx + 2);
        }
    }
    EscapeParse::Invalid(data.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingReporter {
        violations: Mutex<Vec<SigmaViolation>>,
    }

    impl SigmaPolicyReporter for RecordingReporter {
        fn report(&self, violation: SigmaViolation) {
            self.violations
                .lock()
                .expect("violations lock poisoned")
                .push(violation);
        }
    }

    #[test]
    fn osc52_read_is_blocked() {
        let recorder = Arc::new(RecordingReporter::default());
        let reporter: Arc<dyn SigmaPolicyReporter + Send + Sync> = recorder.clone();
        set_sigma_policy_reporter(reporter.clone());
        let seq = b"\x1b]52;;?\x07after";
        let out = sanitize_output(seq, &reporter);
        assert_eq!(out, b"after");
        let violations = recorder
            .violations
            .lock()
            .expect("violations lock poisoned")
            .clone();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].reason, "OSC 52 read blocked");
        assert_eq!(violations[0].direction, SigmaDirection::Output);
        assert_eq!(violations[0].sequence_len, 8);
        set_sigma_policy_reporter(Arc::new(NoopReporter));
    }

    #[test]
    fn osc133_marker_passes_through() {
        let recorder = Arc::new(RecordingReporter::default());
        let reporter: Arc<dyn SigmaPolicyReporter + Send + Sync> = recorder.clone();
        set_sigma_policy_reporter(reporter.clone());
        let seq = b"\x1b]133;A\x07prompt";
        let out = sanitize_output(seq, &reporter);
        assert_eq!(out, seq);
        let violations = recorder
            .violations
            .lock()
            .expect("violations lock poisoned");
        assert!(violations.is_empty());
        set_sigma_policy_reporter(Arc::new(NoopReporter));
    }
}
