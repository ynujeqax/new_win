//! One-line text editing for the Spotlight field (spec: M3 §2.4).

/// The field holds at most this many characters (pasted text is cut).
pub const MAX_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// Typed or pasted text: only its first line, without control characters.
    Insert(String),
    Backspace,
    /// `Ctrl+Backspace`: the word before the caret.
    BackspaceWord,
    Delete,
    Left,
    Right,
    Home,
    End,
    /// `Ctrl+A`: the next edit acts on the whole text; a move clears it.
    SelectAll,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdit {
    chars: Vec<char>,
    /// Char index.
    caret: usize,
    all: bool,
    text: String,
}

impl TextEdit {
    /// True when the text changed.
    pub fn apply(&mut self, e: Edit) -> bool {
        let before = self.chars.len();
        let old = (self.chars.clone(), self.caret);
        if self.all && matches!(e, Edit::Insert(_) | Edit::Backspace | Edit::BackspaceWord | Edit::Delete) {
            self.chars.clear();
            self.caret = 0;
            if !matches!(e, Edit::Insert(_)) {
                self.all = false;
                return self.sync(before > 0);
            }
        }
        self.all = false;
        match e {
            Edit::Insert(s) => {
                let line = s.split(['\r', '\n']).next().unwrap_or("");
                let room = MAX_CHARS.saturating_sub(self.chars.len());
                let add: Vec<char> = line.chars().filter(|c| !c.is_control()).take(room).collect();
                let n = add.len();
                self.chars.splice(self.caret..self.caret, add);
                self.caret += n;
            }
            Edit::Backspace if self.caret > 0 => {
                self.caret -= 1;
                self.chars.remove(self.caret);
            }
            Edit::BackspaceWord => {
                let mut i = self.caret;
                while i > 0 && self.chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                while i > 0 && !self.chars[i - 1].is_whitespace() {
                    i -= 1;
                }
                self.chars.drain(i..self.caret);
                self.caret = i;
            }
            Edit::Delete if self.caret < self.chars.len() => {
                self.chars.remove(self.caret);
            }
            Edit::Left => self.caret = self.caret.saturating_sub(1),
            Edit::Right => self.caret = (self.caret + 1).min(self.chars.len()),
            Edit::Home => self.caret = 0,
            Edit::End => self.caret = self.chars.len(),
            Edit::SelectAll => self.all = !self.chars.is_empty(),
            _ => {}
        }
        let changed = old.0 != self.chars;
        self.sync(changed)
    }

    fn sync(&mut self, changed: bool) -> bool {
        if changed {
            self.text = self.chars.iter().collect();
        }
        changed
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The caret as a UTF-16 position (for DirectWrite hit testing).
    pub fn caret_utf16(&self) -> usize {
        self.chars[..self.caret].iter().map(|c| c.len_utf16()).sum()
    }

    pub fn all_selected(&self) -> bool {
        self.all
    }

    pub fn clear(&mut self) {
        *self = TextEdit::default();
    }
}
