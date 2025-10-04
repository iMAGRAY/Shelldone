use anyhow::Result;
use log::{debug, warn};
use portable_pty::{Child, ChildKiller, ExitStatus, MasterPty, PtySize};
use std::cmp::min;
use std::io::{self, Read, Write};

pub struct SigmaProxyPty {
    inner: Box<dyn MasterPty>,
}

impl SigmaProxyPty {
    pub fn new(inner: Box<dyn MasterPty>) -> Self {
        Self { inner }
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
        Ok(Box::new(SigmaProxyReader::new(reader)))
    }

    fn take_writer(&self) -> Result<Box<dyn Write + Send>> {
        let writer = self.inner.take_writer()?;
        Ok(Box::new(SigmaProxyWriter::new(writer)))
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
}

impl SigmaProxyReader {
    fn new(inner: Box<dyn Read + Send>) -> Self {
        Self { inner }
    }
}

impl Read for SigmaProxyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut tmp = vec![0u8; buf.len()];
        let n = self.inner.read(&mut tmp)?;
        if n == 0 {
            return Ok(0);
        }
        let sanitized = sanitize_output(&tmp[..n]);
        let len = min(sanitized.len(), buf.len());
        buf[..len].copy_from_slice(&sanitized[..len]);
        Ok(len)
    }
}

struct SigmaProxyWriter {
    inner: Box<dyn Write + Send>,
}

impl SigmaProxyWriter {
    fn new(inner: Box<dyn Write + Send>) -> Self {
        Self { inner }
    }
}

impl Write for SigmaProxyWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let sanitized = sanitize_input(buf);
        self.inner.write(&sanitized)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

const ESC: u8 = 0x1B;
const BEL: u8 = 0x07;

fn sanitize_input(data: &[u8]) -> Vec<u8> {
    // TODO: enforce policy-driven filtering for outgoing commands.
    data.to_vec()
}

fn sanitize_output(data: &[u8]) -> Vec<u8> {
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
                    warn!("Filtered escape sequence: {reason}");
                    i += len;
                }
                EscapeParse::Invalid(len) => {
                    warn!("Invalid escape sequence length={len}");
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
        if byte >= 0x40 && byte <= 0x7E {
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
    while idx < data.len() {
        let b = data[idx];
        if b == BEL {
            idx += 1;
            break;
        }
        if b == ESC {
            if idx + 1 < data.len() && data[idx + 1] == b'\\' {
                idx += 2;
                break;
            }
        }
        idx += 1;
    }
    let len = idx.min(data.len());
    let code_value = std::str::from_utf8(&code)
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    match code_value {
        Some(0 | 2 | 4 | 8 | 52 | 133 | 1337) => EscapeParse::Allowed(len),
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
        if byte == ESC {
            if idx + 1 < data.len() && data[idx + 1] == b'\\' {
                return EscapeParse::Allowed(idx + 2);
            }
        }
    }
    EscapeParse::Invalid(data.len())
}
