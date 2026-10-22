//! Keyboard layout on the bar (spec: M2 §2, §4.3): `ENG` / `UKR`, cross-fading on a switch.

use newwin_core::anim::Tween;
use newwin_core::input_lang::display_code;
use newwin_core::items::RightItem;
use newwin_platform::clock::now_ms;
use newwin_platform::input_hook::InputHook;
use newwin_platform::keyboard_layout::{current_layout, switch_next};
use newwin_ui::TextLayout;
use windows::Win32::UI::WindowsAndMessaging::{SetTimer, WM_APP};

use super::App;
use super::apps_view::Target;

pub(super) const WM_APP_LAYOUT: u32 = WM_APP + 10;
pub(super) const LAYOUT_RECHECK_TIMER: usize = 8;
/// Windows applies a layout switch asynchronously; read it a moment later.
const RECHECK_MS: u32 = 50;
const CROSSFADE_MS: u32 = 120;

pub(super) struct LangView {
    pub code: String,
    pub name: String,
    pub prev: Option<String>,
    pub fade: Tween,
    pub text: Option<TextLayout>,
    pub prev_text: Option<TextLayout>,
}

impl App {
    pub(super) fn start_layout(&mut self) {
        self.input_hook = InputHook::start(self.ping, WM_APP_LAYOUT, super::spotlight_view::WM_APP_SPOTLIGHT, self.config.spotlight.win_key).map_err(|e| log::error!("keyboard hook: {e}")).ok();
        self.read_layout();
    }

    /// A layout hotkey, or our own switch: check again shortly.
    pub(super) fn on_layout_hotkey(&mut self) {
        unsafe { SetTimer(Some(self.bar), LAYOUT_RECHECK_TIMER, RECHECK_MS, None) };
    }

    pub(super) fn switch_layout(&mut self) {
        switch_next();
        self.on_layout_hotkey();
    }

    pub(super) fn read_layout(&mut self) {
        let Some(info) = current_layout() else { return };
        let code = display_code(&info.code);
        match self.right.lang.as_mut() {
            None => {
                self.right.lang = Some(LangView { code, name: info.name, prev: None, fade: Tween::at_rest(1.0), text: None, prev_text: None });
            }
            Some(l) if l.code != code => {
                log::debug!("layout {} -> {code}", l.code);
                l.prev = Some(std::mem::replace(&mut l.code, code));
                l.name = info.name;
                l.fade = Tween::at_rest(0.0);
                l.fade.to(1.0, now_ms(), CROSSFADE_MS);
            }
            Some(_) => return,
        }
        // The code's width may differ: lay the right side out again.
        self.refresh();
        if self.tooltip.is_some() && self.hover == Some(Target::Right(RightItem::Layout)) {
            self.show_tooltip();
        }
    }

    pub(super) fn layout_tooltip(&self) -> Option<String> {
        self.right.lang.as_ref().map(|l| l.name.clone()).filter(|n| !n.is_empty())
    }
}
