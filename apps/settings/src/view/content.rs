//! The window content: painting, input, timers and messages.

use super::*;

impl AppContent for SettingsView {
    fn paint(&mut self, p: &mut Painter, now_ms: u64) {
        let texts = match self.texts.take() {
            Some(t) => t,
            None => self.build_texts(),
        };
        self.geo = self.layout(&texts);
        let (d, g) = (self.dpi, &self.geo);
        let s = |v: f32| scale(v, d);
        self.nav.paint(p, &g.nav, &texts.nav, now_ms, d);
        if let Some(r) = g.banner {
            p.fill_round_rect(r, s(BANNER_RADIUS), BANNER_FILL);
            p.stroke_round_rect(r, s(BANNER_RADIUS), BANNER_RIM, s(1.0).round().max(1.0));
            if let Some(t) = texts.banner.as_ref() {
                p.text(t, PointF { x: (r.x + s(BANNER_PAD_X)).round(), y: (r.y + (r.h - t.height()) / 2.0).round() }, theme::TEXT);
            }
            if let (Some(b), Some(t)) = (g.button, texts.button.as_ref()) {
                let fill = if self.button_pressed { theme::ACTIVE_PILL } else if self.button_hover { BUTTON_HOVER } else { BUTTON_FILL };
                p.fill_round_rect(b, b.h / 2.0, fill);
                p.text(t, PointF { x: (b.x + (b.w - t.width()) / 2.0).round(), y: (b.y + (b.h - t.height()) / 2.0).round() }, theme::TEXT_STRONG);
            }
        }
        if let Some(t) = texts.title.as_ref() {
            p.text(t, PointF { x: g.title.x.round(), y: g.title.y.round() }, theme::TEXT_STRONG);
        }
        let body_area = g.body;
        self.body.paint(p, &mut self.text, self.strings, body_area, now_ms, self.blink);
        // The field shows its own focus; a ring outside the body area would sit on the title.
        if let Some(i) = self.focus
            && let Some(&(kind, r)) = self.focusables().get(i)
            && kind != FocusKind::TextField
            && ((i == 0 && self.has_button()) || (r.y >= body_area.y && r.bottom() <= body_area.bottom()))
        {
            let ring = r.inset(-s(FOCUS_GAP));
            p.stroke_round_rect(ring, ring.h.min(ring.w) / 2.0, FOCUS_RING, s(1.0).round().max(1.0));
        }
        if let Some(rect) = self.list_rect()
            && let Some(list) = self.font_list.as_mut()
        {
            list.paint(p, &mut self.text, rect);
        }
        self.texts = Some(texts);
    }

    fn input(&mut self, ev: PanelInput, now_ms: u64) -> PanelReply {
        let repaint = self.route(ev, now_ms);
        PanelReply { repaint, ..PanelReply::default() }
    }

    fn animating(&self, now_ms: u64) -> bool {
        self.nav.animating(now_ms) || self.body.animating(now_ms)
    }

    fn wake_at(&self, now_ms: u64) -> Option<u64> {
        self.body.field.next_blink(now_ms, self.blink)
    }

    fn dpi_changed(&mut self, dpi: u32) {
        self.dpi = dpi;
        self.texts = None;
        self.body.set_dpi(dpi);
        // The list is laid out for the old DPI.
        self.font_list = None;
    }

    fn message(&mut self, msg: u32, wp: WPARAM, _lp: LPARAM, now_ms: u64) -> PanelReply {
        match (msg, wp.0) {
            (DIR_CHANGED, _) => {
                self.set_timer(RELOAD_TIMER, RELOAD_DELAY_MS);
                return PanelReply::default();
            }
            (WM_TIMER, WRITE_TIMER) => {
                self.kill_timer(WRITE_TIMER);
                self.flush();
            }
            (WM_TIMER, RELOAD_TIMER) => {
                self.kill_timer(RELOAD_TIMER);
                self.reload();
            }
            (WM_TIMER, RECHECK_TIMER) => {
                self.kill_timer(RECHECK_TIMER);
                self.check_shell();
            }
            (WM_TIMER, CONFIRM_TIMER) => {
                self.kill_timer(CONFIRM_TIMER);
                self.refresh_armed(now_ms);
            }
            (WM_ACTIVATE, w) => {
                let active = (w & 0xFFFF) as u32 != WA_INACTIVE;
                self.body.field.set_window_active(active);
                if active {
                    self.check_shell();
                }
            }
            _ => return PanelReply::default(),
        }
        PanelReply { repaint: true, ..PanelReply::default() }
    }
}
