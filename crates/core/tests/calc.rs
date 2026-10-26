use newwin_core::calc::{AnswerKind, answer, eval, format_number};

fn math(q: &str) -> Option<String> {
    answer(q).filter(|a| a.kind == AnswerKind::Math).map(|a| a.text)
}

fn conv(q: &str) -> Option<String> {
    answer(q).filter(|a| a.kind == AnswerKind::Conversion).map(|a| a.text)
}

#[test]
fn arithmetic() {
    assert_eq!(math("2+2*3").as_deref(), Some("8"));
    assert_eq!(math("(2+2)*3").as_deref(), Some("12"));
    assert_eq!(math("10/4").as_deref(), Some("2.5"));
    assert_eq!(math("2^10").as_deref(), Some("1024"));
    assert_eq!(math("2^3^2").as_deref(), Some("512"), "right-associative");
    assert_eq!(math("-2^2").as_deref(), Some("-4"));
    assert_eq!(math("7 × 6").as_deref(), Some("42"));
    assert_eq!(math("3x4").as_deref(), Some("12"));
    assert_eq!(math("9 ÷ 3").as_deref(), Some("3"));
    assert_eq!(math("2,5*2").as_deref(), Some("5"), "comma decimal point");
    assert_eq!(math("1/3").as_deref(), Some("0.3333333333"));
    assert_eq!(math("5!").as_deref(), Some("120"));
    assert_eq!(math("2+3=").as_deref(), Some("5"));
}

#[test]
fn percents() {
    assert_eq!(math("15% * 80").as_deref(), Some("12"));
    assert_eq!(math("80*15%").as_deref(), Some("12"));
    assert_eq!(math("100 + 10%").as_deref(), Some("110"), "pocket-calculator percent");
    assert_eq!(math("200 - 25%").as_deref(), Some("150"));
}

#[test]
fn functions_and_constants() {
    assert_eq!(math("sqrt(16)").as_deref(), Some("4"));
    assert_eq!(math("√9 + 1").as_deref(), Some("4"));
    assert_eq!(math("2pi").as_deref(), Some("6.283185307"));
    assert_eq!(math("sin(30)").as_deref(), Some("0.5"), "degrees");
    assert_eq!(math("log(1000)").as_deref(), Some("3"));
    assert_eq!(math("(1+2").as_deref(), Some("3"), "a missing ) at the end while typing");
}

#[test]
fn not_questions() {
    for q in ["", "42", "-5", "steam", "cs2", "3dmark", "Office 365", "Windows 11", "7zip", "1/0", "sqrt(-1)", "2..3", "e", "pi"] {
        assert_eq!(answer(q), None, "{q:?}");
    }
}

#[test]
fn conversions() {
    assert_eq!(conv("10 km in mi").as_deref(), Some("6.21371 mi"));
    assert_eq!(conv("10km to miles").as_deref(), Some("6.21371 mi"));
    assert_eq!(conv("23c in f").as_deref(), Some("73.4 °F"));
    assert_eq!(conv("100 °F to C").as_deref(), Some("37.7778 °C"));
    assert_eq!(conv("0 c in k").as_deref(), Some("273.15 K"));
    assert_eq!(conv("5 kg in lb").as_deref(), Some("11.0231 lb"));
    assert_eq!(conv("1 gib in mb").as_deref(), Some("1073.74 mb"));
    assert_eq!(conv("90 min in h").as_deref(), Some("1.5 h"));
    assert_eq!(conv("2*3 km in m").as_deref(), Some("6000 m"), "the amount may be an expression");
    assert_eq!(conv("5 км в м").as_deref(), Some("5000 m"), "Ukrainian units and separator");
    assert_eq!(conv("100 kmh in mph").as_deref(), Some("62.1371 mph"));
    assert_eq!(conv("5 in in cm").as_deref(), Some("12.7 cm"));
}

#[test]
fn conversions_need_matching_units() {
    assert_eq!(answer("10 km in kg"), None);
    assert_eq!(answer("10 parsecs in km"), None);
    assert_eq!(answer("songs in spotify"), None);
}

#[test]
fn numbers_read_well() {
    assert_eq!(format_number(1234567.89), "1234567.89");
    assert_eq!(format_number(0.1 + 0.2), "0.3");
    assert_eq!(format_number(-0.0), "0");
    assert_eq!(eval("1e3"), None, "no exponent literals: 'e' is the constant");
}
