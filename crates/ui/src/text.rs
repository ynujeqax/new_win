//! Text layout with cached formats and ellipsis trimming.

use std::collections::HashMap;
use std::rc::Rc;

use windows::Win32::Graphics::DirectWrite::*;
use windows::core::{HSTRING, Result, w};

use crate::Gfx;

#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub family: String,
    /// Physical pixels.
    pub size: f32,
    pub weight: u16,
}

pub struct TextLayout {
    pub(crate) layout: IDWriteTextLayout,
    width: f32,
    height: f32,
}

impl TextLayout {
    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }
}

pub struct TextCache {
    gfx: Rc<Gfx>,
    formats: HashMap<(String, u32, u16), IDWriteTextFormat>,
}

impl TextCache {
    pub fn new(gfx: &Rc<Gfx>) -> Self {
        Self { gfx: gfx.clone(), formats: HashMap::new() }
    }

    fn format(&mut self, style: &TextStyle) -> Result<IDWriteTextFormat> {
        let key = (style.family.clone(), style.size.to_bits(), style.weight);
        if let Some(f) = self.formats.get(&key) {
            return Ok(f.clone());
        }
        let dw = self.gfx.dwrite();
        let f = unsafe {
            let f = dw.CreateTextFormat(
                &HSTRING::from(style.family.as_str()),
                None,
                DWRITE_FONT_WEIGHT(style.weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                style.size,
                w!(""),
            )?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let sign = dw.CreateEllipsisTrimmingSign(&f)?;
            let trimming = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
            f.SetTrimming(&trimming, &sign)?;
            f
        };
        self.formats.insert(key, f.clone());
        Ok(f)
    }

    /// Lays out one line; text wider than `max_width` ends with "…". `width()` is the drawn width.
    pub fn layout(&mut self, s: &str, style: &TextStyle, max_width: f32) -> Result<TextLayout> {
        let format = self.format(style)?;
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let layout = self.gfx.dwrite().CreateTextLayout(&wide, &format, max_width.max(0.0), style.size * 2.0)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok(TextLayout { layout, width: m.width.min(max_width), height: m.height })
        }
    }

    /// Like [`Self::layout`], with the UTF-16 range `bold` = (start, len) in semibold (a search
    /// match). A range past the end is clamped.
    pub fn layout_emphasis(&mut self, s: &str, style: &TextStyle, max_width: f32, bold: Option<(usize, usize)>) -> Result<TextLayout> {
        let format = self.format(style)?;
        let wide: Vec<u16> = s.encode_utf16().collect();
        unsafe {
            let layout = self.gfx.dwrite().CreateTextLayout(&wide, &format, max_width.max(0.0), style.size * 2.0)?;
            if let Some((start, len)) = bold {
                let start = start.min(wide.len());
                let len = len.min(wide.len() - start);
                if len > 0 {
                    let range = DWRITE_TEXT_RANGE { startPosition: start as u32, length: len as u32 };
                    layout.SetFontWeight(DWRITE_FONT_WEIGHT(i32::from(crate::theme::WEIGHT_SEMIBOLD)), range)?;
                }
            }
            let mut m = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut m)?;
            Ok(TextLayout { layout, width: m.width.min(max_width), height: m.height })
        }
    }

    /// Width the text needs without trimming.
    pub fn natural_width(&mut self, s: &str, style: &TextStyle) -> Result<f32> {
        Ok(self.layout(s, style, 100_000.0)?.width())
    }
}
