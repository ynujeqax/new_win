//! Shell icons in a helper process (1.0.1): fetching them loads shell components (XAML, the Start
//! tile cache, thread pools) that never unload, so `NewWin.exe --icon-helper` does it and quits
//! when it is not needed. The shell talks to it over stdin/stdout:
//!
//! - request: `px \t source \t fallback \n` (an empty fallback is none)
//! - reply: `width height \n` and `width * height * 4` bytes of premultiplied BGRA (`0 0` = no icon)

use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use newwin_core::app_icon::IconImage;

use crate::app_icon::shell_icon;

/// The largest icon a reply may claim (a broken helper must not make the shell allocate GBs).
const MAX_SIDE: u32 = 256;
/// How long the shell waits for one reply before it takes the helper for hung and ends it: a
/// stuck shell extension must not hold up the worker (launches and saves queue behind icons).
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// How long a closed helper gets to exit by itself before it is ended.
const EXIT_GRACE: Duration = Duration::from_secs(1);

pub fn write_request(w: &mut impl Write, source: &str, fallback: Option<&str>, px: u32) -> io::Result<()> {
    let bad = |s: &str| s.contains(['\t', '\n', '\r']);
    if bad(source) || fallback.is_some_and(bad) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "an icon source with a tab or a line break"));
    }
    writeln!(w, "{px}\t{source}\t{}", fallback.unwrap_or(""))
}

/// The next request, `None` at the end of the input.
pub fn read_request(r: &mut impl BufRead) -> io::Result<Option<(u32, String, Option<String>)>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.trim_end_matches(['\r', '\n']).splitn(3, '\t');
    let px = parts.next().and_then(|p| p.parse().ok()).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad size"))?;
    let source = parts.next().unwrap_or("").to_string();
    let fallback = parts.next().filter(|f| !f.is_empty()).map(str::to_string);
    Ok(Some((px, source, fallback)))
}

pub fn write_reply(w: &mut impl Write, img: Option<&IconImage>) -> io::Result<()> {
    match img {
        Some(i) => {
            writeln!(w, "{} {}", i.width, i.height)?;
            w.write_all(&i.bgra)
        }
        None => writeln!(w, "0 0"),
    }
}

pub fn read_reply(r: &mut impl BufRead) -> io::Result<Option<IconImage>> {
    let mut line = String::new();
    if r.read_line(&mut line)? == 0 {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the icon helper went away"));
    }
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "bad icon reply");
    let mut it = line.split_whitespace().map(|n| n.parse::<u32>());
    let (Some(Ok(width)), Some(Ok(height))) = (it.next(), it.next()) else { return Err(bad()) };
    if width == 0 || height == 0 {
        return Ok(None);
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(bad());
    }
    let mut bgra = vec![0u8; (width * height * 4) as usize];
    r.read_exact(&mut bgra)?;
    Ok(Some(IconImage { width, height, bgra }))
}

/// The helper's loop: answers requests until the input ends (the shell closed it or quit).
/// Needs COM on the calling thread.
pub fn serve(mut input: impl BufRead, mut output: impl Write) {
    while let Ok(Some((px, source, fallback))) = read_request(&mut input) {
        let img = shell_icon(&source, px).or_else(|| fallback.and_then(|f| shell_icon(&f, px)));
        // The shell refuses a larger reply as broken: send none instead.
        let img = img.filter(|i| i.width <= MAX_SIDE && i.height <= MAX_SIDE);
        if write_reply(&mut output, img.as_ref()).and_then(|_| output.flush()).is_err() {
            return;
        }
    }
}

struct Running {
    child: Child,
    stdin: ChildStdin,
    /// Replies, read on a thread of their own so that one can be waited for with a timeout.
    replies: mpsc::Receiver<io::Result<Option<IconImage>>>,
}

/// The shell's side: starts `exe --icon-helper` on the first request, stops it on [`close`].
///
/// [`close`]: IconClient::close
pub struct IconClient {
    exe: PathBuf,
    args: Vec<String>,
    timeout: Duration,
    child: Option<Running>,
}

impl IconClient {
    pub fn new(exe: PathBuf) -> Self {
        Self::with_command(exe, vec!["--icon-helper".into()], REPLY_TIMEOUT)
    }

    /// Any command that speaks the protocol on stdin/stdout, with its own reply timeout (tests).
    pub fn with_command(exe: PathBuf, args: Vec<String>, timeout: Duration) -> Self {
        Self { exe, args, timeout, child: None }
    }

    pub fn running(&self) -> bool {
        self.child.is_some()
    }

    /// The icon of `source` (else `fallback`); `None` when there is none or the helper failed.
    pub fn fetch(&mut self, source: &str, fallback: Option<&str>, px: u32) -> Option<IconImage> {
        // One retry with a new helper: the old one may have died. A hung one is not retried.
        for _ in 0..2 {
            if self.child.is_none() {
                self.child = self.start().map_err(|e| log::warn!("cannot start the icon helper: {e}")).ok();
            }
            let run = self.child.as_mut()?;
            let reply = write_request(&mut run.stdin, source, fallback, px).and_then(|_| run.stdin.flush()).and_then(|_| {
                match run.replies.recv_timeout(self.timeout) {
                    Ok(r) => r,
                    Err(mpsc::RecvTimeoutError::Timeout) => Err(io::Error::new(io::ErrorKind::TimedOut, "no reply")),
                    Err(mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the icon helper went away")),
                }
            });
            match reply {
                Ok(img) => return img,
                Err(e) if e.kind() == io::ErrorKind::InvalidInput => return None,
                Err(e) if e.kind() == io::ErrorKind::TimedOut => {
                    log::warn!("icon helper: no reply for {source} in {} ms, ending it", self.timeout.as_millis());
                    self.kill();
                    return None;
                }
                Err(e) => {
                    log::warn!("icon helper: {e}");
                    self.close();
                }
            }
        }
        None
    }

    fn start(&self) -> io::Result<Running> {
        let mut child = Command::new(&self.exe).args(&self.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| io::Error::other("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| io::Error::other("no stdout"))?;
        let (tx, replies) = mpsc::channel();
        // Ends with the helper's output (it exited or was ended) or once nobody waits for replies.
        let reader = std::thread::Builder::new().name("newwin-icon-reader".into()).spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let reply = read_reply(&mut stdout);
                let failed = reply.is_err();
                if tx.send(reply).is_err() || failed {
                    return;
                }
            }
        });
        if let Err(e) = reader {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        log::debug!("icon helper started pid={}", child.id());
        Ok(Running { child, stdin, replies })
    }

    /// Closes the helper's input; it finishes and exits by itself, or is ended after a moment.
    pub fn close(&mut self) {
        if let Some(Running { mut child, stdin, .. }) = self.child.take() {
            drop(stdin);
            let deadline = Instant::now() + EXIT_GRACE;
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            if matches!(child.try_wait(), Ok(None)) {
                log::warn!("icon helper did not exit, ending it");
                let _ = child.kill();
            }
            let _ = child.wait();
            log::debug!("icon helper stopped");
        }
    }

    /// Ends a helper that stopped answering.
    fn kill(&mut self) {
        if let Some(Running { mut child, .. }) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for IconClient {
    fn drop(&mut self) {
        self.close();
    }
}
