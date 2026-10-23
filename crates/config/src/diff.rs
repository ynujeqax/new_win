//! Which live actions a config change needs.

use crate::Config;

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Changes {
    /// appearance.height | margin | corner_radius
    pub geometry: bool,
    /// bar.mode | show/hide delays | trigger_height_px | ignore_fullscreen
    pub visibility: bool,
    /// bar.mode alone (appbar register/unregister)
    pub mode: bool,
    /// appearance.font_family | font_size
    pub fonts: bool,
    /// appearance.glass
    pub glass: bool,
    /// appearance.date_format | show_seconds
    pub clock: bool,
    /// items.right | items.hidden
    pub items: bool,
    /// spotlight.win_key
    pub win_key: bool,
    /// spotlight.max_results (applies on next open)
    pub spotlight: bool,
    /// tray.order | tray.hidden
    pub tray: bool,
    /// system.start_with_windows
    pub autostart: bool,
}

impl Changes {
    pub fn any(&self) -> bool {
        *self != Changes::default()
    }
}

/// Compares two configs field by field; `version` is ignored.
pub fn changes(old: &Config, new: &Config) -> Changes {
    let (ob, nb) = (&old.bar, &new.bar);
    let (oa, na) = (&old.appearance, &new.appearance);
    let mode = ob.mode != nb.mode;
    Changes {
        geometry: oa.height != na.height || oa.margin != na.margin || oa.corner_radius != na.corner_radius,
        visibility: mode
            || ob.show_delay_ms != nb.show_delay_ms
            || ob.hide_delay_ms != nb.hide_delay_ms
            || ob.trigger_height_px != nb.trigger_height_px
            || ob.ignore_fullscreen != nb.ignore_fullscreen,
        mode,
        fonts: oa.font_family != na.font_family || oa.font_size != na.font_size,
        glass: oa.glass != na.glass,
        clock: oa.date_format != na.date_format || oa.show_seconds != na.show_seconds,
        items: old.items != new.items,
        win_key: old.spotlight.win_key != new.spotlight.win_key,
        spotlight: old.spotlight.max_results != new.spotlight.max_results,
        tray: old.tray != new.tray,
        autostart: old.system.start_with_windows != new.system.start_with_windows,
    }
}
