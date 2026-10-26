//! What is playing, from Windows' media sessions (Spotify, browsers, any player that shows in
//! the volume flyout). A thread of its own owns the session manager: its events only wake the
//! thread, which reads the current session and posts what changed; commands go the same way.

use std::sync::mpsc;
use std::thread::JoinHandle;

use newwin_core::media::{MediaCmd, MediaInfo};
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{Interface, Result};

enum Msg {
    Changed,
    Cmd(MediaCmd),
    Quit,
}

pub struct MediaWatch {
    tx: mpsc::Sender<Msg>,
    thread: Option<JoinHandle<()>>,
}

/// # Safety
/// `lp` must be the LPARAM of a message posted by [`MediaWatch`], taken exactly once.
pub unsafe fn take_media(lp: LPARAM) -> Box<Option<MediaInfo>> {
    unsafe { Box::from_raw(lp.0 as *mut Option<MediaInfo>) }
}

impl MediaWatch {
    /// Posts `msg` to `notify` with a `Box<Option<MediaInfo>>` (take it with [`take_media`]) at
    /// start and whenever what is playing changes.
    pub fn start(notify: HWND, msg: u32) -> MediaWatch {
        let (tx, rx) = mpsc::channel();
        let wake = tx.clone();
        let notify = notify.0 as isize;
        let thread = std::thread::Builder::new()
            .name("newwin-media".into())
            .spawn(move || {
                let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
                if let Err(e) = run(&rx, &wake, notify, msg) {
                    log::warn!("media sessions unavailable: {e}");
                }
                unsafe { CoUninitialize() };
            })
            .map_err(|e| log::error!("media thread: {e}"))
            .ok();
        MediaWatch { tx, thread }
    }

    pub fn send(&self, cmd: MediaCmd) {
        let _ = self.tx.send(Msg::Cmd(cmd));
    }
}

impl Drop for MediaWatch {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn post(notify: isize, msg: u32, info: Option<MediaInfo>) {
    let boxed = Box::into_raw(Box::new(info));
    let ok = unsafe { PostMessageW(Some(HWND(notify as *mut _)), msg, WPARAM(0), LPARAM(boxed as isize)) };
    if ok.is_err() {
        drop(unsafe { Box::from_raw(boxed) });
    }
}

/// The session we listen to, with its event tokens.
struct Watched {
    session: Session,
    props: i64,
    playback: i64,
}

impl Watched {
    fn stop(self) {
        let _ = self.session.RemoveMediaPropertiesChanged(self.props);
        let _ = self.session.RemovePlaybackInfoChanged(self.playback);
    }
}

fn run(rx: &mpsc::Receiver<Msg>, wake: &mpsc::Sender<Msg>, notify: isize, msg: u32) -> Result<()> {
    let manager = Manager::RequestAsync()?.join()?;
    let w = wake.clone();
    let t1 = manager.CurrentSessionChanged(&TypedEventHandler::new(move |_, _| {
        let _ = w.send(Msg::Changed);
        Ok(())
    }))?;
    let w = wake.clone();
    let t2 = manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = w.send(Msg::Changed);
        Ok(())
    }))?;
    let mut watched: Option<Watched> = None;
    let mut last: Option<Option<MediaInfo>> = None;
    loop {
        let session = manager.GetCurrentSession().ok();
        let same = match (&watched, &session) {
            (Some(w), Some(s)) => w.session.as_raw() == s.as_raw(),
            (None, None) => true,
            _ => false,
        };
        if !same {
            if let Some(w) = watched.take() {
                w.stop();
            }
            watched = session.as_ref().and_then(|s| watch(s, wake).ok());
        }
        let info = session.as_ref().and_then(|s| read(s).ok()).filter(MediaInfo::shown);
        if last.as_ref() != Some(&info) {
            log::debug!("media {info:?}");
            post(notify, msg, info.clone());
            last = Some(info);
        }
        // Wait for something to happen; a burst of events reads the session once.
        let mut next = rx.recv();
        while let Ok(Msg::Changed) = next {
            match rx.recv_timeout(std::time::Duration::from_millis(30)) {
                Ok(m) => next = Ok(m),
                Err(_) => break,
            }
        }
        match next {
            Ok(Msg::Changed) => {}
            Ok(Msg::Cmd(c)) => {
                if let Some(s) = &session {
                    command(s, c);
                }
            }
            Ok(Msg::Quit) | Err(_) => break,
        }
    }
    if let Some(w) = watched.take() {
        w.stop();
    }
    let _ = manager.RemoveCurrentSessionChanged(t1);
    let _ = manager.RemoveSessionsChanged(t2);
    Ok(())
}

fn watch(s: &Session, wake: &mpsc::Sender<Msg>) -> Result<Watched> {
    let w = wake.clone();
    let props = s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
        let _ = w.send(Msg::Changed);
        Ok(())
    }))?;
    let w = wake.clone();
    let playback = s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
        let _ = w.send(Msg::Changed);
        Ok(())
    }))?;
    Ok(Watched { session: s.clone(), props, playback })
}

fn read(s: &Session) -> Result<MediaInfo> {
    let props = s.TryGetMediaPropertiesAsync()?.join()?;
    let playback = s.GetPlaybackInfo()?;
    let controls = playback.Controls()?;
    Ok(MediaInfo {
        title: props.Title()?.to_string(),
        artist: props.Artist()?.to_string(),
        app: s.SourceAppUserModelId()?.to_string(),
        playing: playback.PlaybackStatus()? == Status::Playing,
        can_toggle: controls.IsPlayPauseToggleEnabled()?,
        can_prev: controls.IsPreviousEnabled()?,
        can_next: controls.IsNextEnabled()?,
    })
}

fn command(s: &Session, c: MediaCmd) {
    let r = match c {
        MediaCmd::PlayPause => s.TryTogglePlayPauseAsync().and_then(|a| a.join()),
        MediaCmd::Next => s.TrySkipNextAsync().and_then(|a| a.join()),
        MediaCmd::Prev => s.TrySkipPreviousAsync().and_then(|a| a.join()),
    };
    if let Err(e) = r {
        log::warn!("media {c:?}: {e}");
    }
}
