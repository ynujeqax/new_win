//! Spotlight's answers: arithmetic ("2+2*3", "15% * 80", "sqrt(2)") and unit conversions
//! ("10 km in mi", "23c to f"). Offline, no allocation-heavy parsing; `answer` is called on every
//! keystroke.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerKind {
    Math,
    Conversion,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub kind: AnswerKind,
    /// What is shown and copied: "8", "6.2137 mi".
    pub text: String,
}

/// The answer to a query, when it is a calculation or a conversion.
pub fn answer(query: &str) -> Option<Answer> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }
    if let Some(text) = convert(q) {
        return Some(Answer { kind: AnswerKind::Conversion, text });
    }
    // A lone number or word is not a question.
    if !q.chars().any(|c| c.is_ascii_digit()) || !q.chars().any(|c| "+-*/×÷^%()!√".contains(c) || c.is_alphabetic()) {
        return None;
    }
    let text = format_number(eval(q)?);
    // "-5" or "(3)" answer themselves.
    let bare: String = q.chars().filter(|c| !c.is_whitespace() && !"()+".contains(*c)).collect();
    (bare != text).then_some(Answer { kind: AnswerKind::Math, text })
}

/// Evaluates an arithmetic expression: + - * / × ÷ ^ % !, parentheses, `sqrt`, `sin`… , `pi`, `e`.
/// A comma is a decimal point ("2,5"). `None` for anything else or a non-finite result.
pub fn eval(s: &str) -> Option<f64> {
    let tokens = tokenize(s)?;
    let mut p = Parser { t: &tokens, i: 0 };
    let v = p.expr()?;
    (p.i == tokens.len() && v.is_finite()).then_some(v)
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Op(char),
    Ident(String),
}

fn tokenize(s: &str) -> Option<Vec<Tok>> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() || c == '\u{a0}' || c == '=' && i == chars.len() - 1 {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' || c == ',') && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit()) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
                i += 1;
            }
            let lit: String = chars[start..i].iter().map(|&c| if c == ',' { '.' } else { c }).collect();
            if lit.matches('.').count() > 1 {
                return None;
            }
            out.push(Tok::Num(lit.parse().ok()?));
        } else if c == 'x' && matches!(out.last(), Some(Tok::Num(_) | Tok::Op(')'))) && !chars.get(i + 1).is_some_and(|n| n.is_alphabetic()) {
            // "3x4", "3 x 4": times.
            out.push(Tok::Op('*'));
            i += 1;
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_alphanumeric() {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect::<String>().to_lowercase()));
        } else {
            let op = match c {
                '×' | '·' => '*',
                '÷' | ':' => '/',
                '−' | '–' => '-',
                '+' | '-' | '*' | '/' | '^' | '%' | '(' | ')' | '!' | '√' => c,
                _ => return None,
            };
            out.push(Tok::Op(op));
            i += 1;
        }
    }
    Some(out)
}

struct Parser<'a> {
    t: &'a [Tok],
    i: usize,
}

impl Parser<'_> {
    fn peek_op(&self) -> Option<char> {
        match self.t.get(self.i) {
            Some(Tok::Op(c)) => Some(*c),
            _ => None,
        }
    }

    fn eat(&mut self, op: char) -> bool {
        let hit = self.peek_op() == Some(op);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            if self.eat('+') {
                v += self.percent_of(v)?;
            } else if self.eat('-') {
                v -= self.percent_of(v)?;
            } else {
                return Some(v);
            }
        }
    }

    /// The right side of `a + b`: "100 + 10%" adds 10 % of 100, as on a pocket calculator.
    fn percent_of(&mut self, base: f64) -> Option<f64> {
        let start = self.i;
        let v = self.term()?;
        let ends_in_percent = self.i > start && self.t[self.i - 1] == Tok::Op('%') && self.i - start == 2;
        Some(if ends_in_percent { base * v } else { v })
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.unary()?;
        loop {
            if self.eat('*') {
                v *= self.unary()?;
            } else if self.eat('/') {
                let d = self.unary()?;
                if d == 0.0 {
                    return None;
                }
                v /= d;
            } else if matches!(self.t.get(self.i), Some(Tok::Num(_) | Tok::Ident(_))) || self.peek_op() == Some('(') || self.peek_op() == Some('√') {
                // Implicit multiplication: "2pi", "3(4+1)", "2√9".
                v *= self.unary()?;
            } else {
                return Some(v);
            }
        }
    }

    fn unary(&mut self) -> Option<f64> {
        if self.eat('-') {
            return Some(-self.unary()?);
        }
        if self.eat('+') {
            return self.unary();
        }
        if self.eat('√') {
            let v = self.unary()?;
            return (v >= 0.0).then(|| v.sqrt());
        }
        self.power()
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.postfix()?;
        if self.eat('^') {
            let exp = self.unary()?;
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn postfix(&mut self) -> Option<f64> {
        let mut v = self.primary()?;
        loop {
            if self.eat('%') {
                v /= 100.0;
            } else if self.eat('!') {
                v = factorial(v)?;
            } else {
                return Some(v);
            }
        }
    }

    fn primary(&mut self) -> Option<f64> {
        match self.t.get(self.i)?.clone() {
            Tok::Num(n) => {
                self.i += 1;
                Some(n)
            }
            Tok::Op('(') => {
                self.i += 1;
                let v = self.expr()?;
                // A missing ")" at the end is forgiven while typing.
                if !self.eat(')') && self.i < self.t.len() {
                    return None;
                }
                Some(v)
            }
            Tok::Ident(name) => {
                self.i += 1;
                match name.as_str() {
                    "pi" | "π" => Some(std::f64::consts::PI),
                    "e" => Some(std::f64::consts::E),
                    f => {
                        let arg = self.postfix()?;
                        apply(f, arg)
                    }
                }
            }
            Tok::Op(_) => None,
        }
    }
}

fn apply(f: &str, x: f64) -> Option<f64> {
    let deg = x.to_radians();
    Some(match f {
        "sqrt" => {
            if x < 0.0 {
                return None;
            }
            x.sqrt()
        }
        "abs" => x.abs(),
        "round" => x.round(),
        "floor" => x.floor(),
        "ceil" => x.ceil(),
        "ln" if x > 0.0 => x.ln(),
        "log" if x > 0.0 => x.log10(),
        // Degrees, as people type them into a launcher.
        "sin" => deg.sin(),
        "cos" => deg.cos(),
        "tan" => deg.tan(),
        _ => return None,
    })
}

fn factorial(n: f64) -> Option<f64> {
    if n < 0.0 || n.fract() != 0.0 || n > 170.0 {
        return None;
    }
    Some((1..=n as u64).fold(1.0, |a, k| a * k as f64))
}

/// Up to 10 significant digits, no trailing zeros, no exponent for everyday sizes: "0.3333333333",
/// "1234567.89", "1e+21".
pub fn format_number(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    if v.abs() >= 1e15 || (v != 0.0 && v.abs() < 1e-9) {
        return format!("{v:e}");
    }
    let digits = 10i32 - (v.abs().log10().floor() as i32 + 1).max(0);
    let s = format!("{:.*}", digits.clamp(0, 10) as usize, v);
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dim {
    Length,
    Mass,
    Temperature,
    Volume,
    Area,
    Speed,
    Data,
    Time,
}

/// (names, dimension, factor to the base unit). Temperature uses its own formulas.
const UNITS: &[(&[&str], Dim, f64)] = &[
    (&["mm", "millimeter", "millimeters", "мм"], Dim::Length, 0.001),
    (&["cm", "centimeter", "centimeters", "см"], Dim::Length, 0.01),
    (&["m", "meter", "meters", "metre", "metres", "м"], Dim::Length, 1.0),
    (&["km", "kilometer", "kilometers", "км"], Dim::Length, 1000.0),
    (&["in", "inch", "inches", "\""], Dim::Length, 0.0254),
    (&["ft", "foot", "feet", "'"], Dim::Length, 0.3048),
    (&["yd", "yard", "yards"], Dim::Length, 0.9144),
    (&["mi", "mile", "miles", "миля", "милі", "миль"], Dim::Length, 1609.344),
    (&["nmi"], Dim::Length, 1852.0),
    (&["mg", "мг"], Dim::Mass, 0.001),
    (&["g", "gram", "grams", "г"], Dim::Mass, 1.0),
    (&["kg", "kilogram", "kilograms", "кг"], Dim::Mass, 1000.0),
    (&["t", "ton", "tons", "tonne", "tonnes", "т"], Dim::Mass, 1_000_000.0),
    (&["oz", "ounce", "ounces"], Dim::Mass, 28.349523125),
    (&["lb", "lbs", "pound", "pounds"], Dim::Mass, 453.59237),
    (&["st", "stone"], Dim::Mass, 6350.29318),
    (&["c", "°c", "celsius", "℃"], Dim::Temperature, 0.0),
    (&["f", "°f", "fahrenheit", "℉"], Dim::Temperature, 1.0),
    (&["k", "kelvin"], Dim::Temperature, 2.0),
    (&["ml", "milliliter", "milliliters", "мл"], Dim::Volume, 0.001),
    (&["l", "liter", "liters", "litre", "litres", "л"], Dim::Volume, 1.0),
    (&["gal", "gallon", "gallons"], Dim::Volume, 3.785411784),
    (&["qt", "quart", "quarts"], Dim::Volume, 0.946352946),
    (&["pt", "pint", "pints"], Dim::Volume, 0.473176473),
    (&["cup", "cups"], Dim::Volume, 0.2365882365),
    (&["floz"], Dim::Volume, 0.0295735295625),
    (&["m2", "м2"], Dim::Area, 1.0),
    (&["km2", "км2"], Dim::Area, 1_000_000.0),
    (&["ft2", "sqft"], Dim::Area, 0.09290304),
    (&["ha", "hectare", "hectares", "га"], Dim::Area, 10_000.0),
    (&["acre", "acres"], Dim::Area, 4046.8564224),
    (&["m/s", "mps"], Dim::Speed, 1.0),
    (&["km/h", "kmh", "kph", "км/год"], Dim::Speed, 1.0 / 3.6),
    (&["mph"], Dim::Speed, 0.44704),
    (&["knot", "knots", "kn"], Dim::Speed, 0.514444),
    (&["b", "byte", "bytes"], Dim::Data, 1.0),
    (&["kb", "kilobyte", "kilobytes"], Dim::Data, 1e3),
    (&["mb", "megabyte", "megabytes"], Dim::Data, 1e6),
    (&["gb", "gigabyte", "gigabytes"], Dim::Data, 1e9),
    (&["tb", "terabyte", "terabytes"], Dim::Data, 1e12),
    (&["kib"], Dim::Data, 1024.0),
    (&["mib"], Dim::Data, 1_048_576.0),
    (&["gib"], Dim::Data, 1_073_741_824.0),
    (&["tib"], Dim::Data, 1_099_511_627_776.0),
    (&["ms", "мс"], Dim::Time, 0.001),
    (&["s", "sec", "secs", "second", "seconds", "с"], Dim::Time, 1.0),
    (&["min", "mins", "minute", "minutes", "хв"], Dim::Time, 60.0),
    (&["h", "hr", "hrs", "hour", "hours", "год"], Dim::Time, 3600.0),
    (&["d", "day", "days", "дн", "днів"], Dim::Time, 86400.0),
    (&["week", "weeks", "тиж"], Dim::Time, 604_800.0),
];

fn unit(name: &str) -> Option<(usize, Dim, f64)> {
    let n = name.trim().to_lowercase();
    let n = n.trim_start_matches('°');
    let n = if n.is_empty() { name.trim() } else { n };
    UNITS.iter().enumerate().find_map(|(i, (names, dim, f))| names.iter().any(|x| x.trim_start_matches('°') == n).then_some((i, *dim, *f)))
}

/// The short name shown with a result.
fn symbol(i: usize) -> &'static str {
    match UNITS[i].1 {
        Dim::Temperature => ["°C", "°F", "K"][UNITS[i].2 as usize],
        _ => UNITS[i].0[0],
    }
}

/// "<amount> <unit> in|to|в|у|= <unit>", amount may be an expression ("2*3 km in m").
fn convert(q: &str) -> Option<String> {
    let lower = q.to_lowercase();
    let (left, right) = [" in ", " to ", " в ", " у ", " = ", " into "].iter().find_map(|sep| lower.rsplit_once(sep))?;
    let (to_i, to_dim, to_f) = unit(right)?;
    // Split the amount from its unit: the unit is the trailing run of non-digit characters.
    let left = left.trim();
    let split = left.char_indices().rev().find(|(_, c)| c.is_ascii_digit() || *c == ')').map(|(i, c)| i + c.len_utf8())?;
    let (amount, from) = left.split_at(split);
    let (_, from_dim, from_f) = unit(from)?;
    if from_dim != to_dim {
        return None;
    }
    let v = eval(amount)?;
    let out = if from_dim == Dim::Temperature {
        let c = match from_f as u8 {
            0 => v,
            1 => (v - 32.0) * 5.0 / 9.0,
            _ => v - 273.15,
        };
        match to_f as u8 {
            0 => c,
            1 => c * 9.0 / 5.0 + 32.0,
            _ => c + 273.15,
        }
    } else {
        v * from_f / to_f
    };
    out.is_finite().then(|| format!("{} {}", format_number(round_sig(out, 6)), symbol(to_i)))
}

/// `v` to `n` significant digits: conversions are approximate, "6.21371 mi" reads better than
/// "6.213711922 mi".
fn round_sig(v: f64, n: i32) -> f64 {
    if v == 0.0 {
        return 0.0;
    }
    let scale = 10f64.powi(n - 1 - v.abs().log10().floor() as i32);
    (v * scale).round() / scale
}
