//! Number formats: Excel format codes (`#,##0`, `0.0%`, `_-* # ##0 "₽"_-;…`, `General`) as chart
//! axes and data labels show them, in the chart's language (digit grouping and decimal mark).

/// Digit grouping and decimal mark for a language tag (`ru-RU`, `de-DE`, `en-US`).
fn marks(lang: Option<&str>) -> (char, char) {
    let l = lang.unwrap_or("en").split(['-', '_']).next().unwrap_or("en").to_ascii_lowercase();
    match l.as_str() {
        // Space-grouped, comma decimal.
        "ru" | "uk" | "be" | "kk" | "fr" | "pl" | "cs" | "sk" | "sv" | "fi" | "nb" | "nn" | "no" | "bg" | "lt" | "lv" | "et" | "hu" | "uz" | "hy"
        | "ka" | "az" => ('\u{a0}', ','),
        // Dot-grouped, comma decimal.
        "de" | "es" | "it" | "pt" | "nl" | "da" | "tr" | "el" | "ro" | "id" | "sr" | "hr" | "sl" | "vi" => ('.', ','),
        _ => (',', '.'),
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    /// Text shown as is.
    Lit(String),
    /// A digit placeholder: `0`, `#` or `?`.
    Digit(char),
    Dot,
    Comma,
    Percent,
}

/// A section's tokens (`[$₽-419]` gives its symbol; colours, conditions and locales drop out).
fn tokens(section: &str) -> Vec<Tok> {
    let mut out = vec![];
    let mut it = section.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut s = String::new();
                for d in it.by_ref() {
                    if d == '"' {
                        break;
                    }
                    s.push(d);
                }
                out.push(Tok::Lit(s));
            }
            '\\' => {
                if let Some(d) = it.next() {
                    out.push(Tok::Lit(d.to_string()));
                }
            }
            // Padding the width of the next character: a space.
            '_' => {
                it.next();
                out.push(Tok::Lit(" ".into()));
            }
            // Repeat-to-fill: nothing on an axis.
            '*' => {
                it.next();
            }
            '[' => {
                let mut s = String::new();
                for d in it.by_ref() {
                    if d == ']' {
                        break;
                    }
                    s.push(d);
                }
                if let Some(sym) = s.strip_prefix('$') {
                    let sym = sym.split('-').next().unwrap_or("");
                    if !sym.is_empty() {
                        out.push(Tok::Lit(sym.to_string()));
                    }
                }
            }
            '0' | '#' | '?' => out.push(Tok::Digit(c)),
            '.' => out.push(Tok::Dot),
            ',' => out.push(Tok::Comma),
            '%' => out.push(Tok::Percent),
            '@' => {}
            _ => out.push(Tok::Lit(c.to_string())),
        }
    }
    out
}

/// The format's sections split at `;` (outside quotes, brackets and escapes).
fn sections(code: &str) -> Vec<&str> {
    let mut out = vec![];
    let (mut start, mut quoted, mut bracket, mut escaped) = (0, false, false, false);
    for (i, c) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if !quoted => escaped = true,
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                out.push(code.get(start..i).unwrap_or(""));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(code.get(start..).unwrap_or(""));
    out
}

/// `v` in the General format: up to 10 significant digits, no grouping.
fn general(v: f64, dec: char) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    let s = if !(1e-9..1e11).contains(&a) {
        format!("{v:.5E}")
    } else {
        let digits = (10 - (a.log10().floor() as i32 + 1)).clamp(0, 10) as usize;
        let s = format!("{v:.digits$}");
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    };
    s.replace('.', &dec.to_string())
}

/// Group an integer's digits in threes with `sep`.
fn group(digits: &str, sep: char) -> String {
    let n = digits.chars().count();
    let mut out = String::new();
    for (i, d) in digits.chars().enumerate() {
        if i > 0 && (n - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(d);
    }
    out
}

/// `v` formatted with the Excel number format `code` in language `lang` (axis labels: the text
/// section of a format doesn't apply).
pub fn format(code: &str, v: f64, lang: Option<&str>) -> String {
    let (sep, dec) = marks(lang);
    if !v.is_finite() {
        return String::new();
    }
    let secs = sections(code);
    // The section for the value's sign; a negative or zero section shows the value unsigned.
    let (sec, v, minus) = match (secs.as_slice(), v) {
        ([_, neg, ..], v) if v < 0.0 => (*neg, -v, false),
        ([_, _, zero, ..], v) if v == 0.0 => (*zero, v, false),
        ([pos, ..], v) => (*pos, v.abs(), v < 0.0),
        ([], v) => ("General", v.abs(), v < 0.0),
    };
    if sec.trim().eq_ignore_ascii_case("general") || sec.trim().is_empty() {
        let s = general(v, dec);
        return if minus { format!("-{s}") } else { s };
    }
    let toks = tokens(sec);
    let first = toks.iter().position(|t| matches!(t, Tok::Digit(_)));
    let last = toks.iter().rposition(|t| matches!(t, Tok::Digit(_)));
    let (Some(first), Some(last)) = (first, last) else {
        // No digits: just the section's text ("-" for zero, a label).
        let s: String = toks.iter().filter_map(|t| if let Tok::Lit(s) = t { Some(s.as_str()) } else { None }).collect();
        return s.trim().to_string();
    };
    let percent = toks.contains(&Tok::Percent);
    let lits = |ts: &[Tok]| -> String {
        ts.iter()
            .filter_map(|t| match t {
                Tok::Lit(s) => Some(s.clone()),
                Tok::Percent => Some("%".into()),
                _ => None,
            })
            .collect()
    };
    // Scientific notation (`0.00E+00`, `0.00E-00`): the exponent's digit placeholders are
    // not fractional digits, and the `E` is not a literal. Parse it explicitly; a quoted
    // or stray `E` only counts when what follows is a sign and/or at least two exponent
    // digits ending at the format's last digit placeholder, as in real format codes.
    let sci = (first + 1..last).find_map(|i| {
        let Tok::Lit(marker) = toks.get(i)? else { return None };
        if marker.as_str() != "E" && marker.as_str() != "e" {
            return None;
        }
        let rest = toks.get(i + 1..=last)?;
        let (always_sign, exp_toks) = match rest.first() {
            Some(Tok::Lit(s)) if s == "+" => (true, rest.get(1..)?),
            Some(Tok::Lit(s)) if s == "-" => (false, rest.get(1..)?),
            _ => (false, rest),
        };
        if exp_toks.is_empty() || !exp_toks.iter().all(|t| matches!(t, Tok::Digit(_))) {
            return None;
        }
        let signed_or_wide = always_sign || matches!(rest.first(), Some(Tok::Lit(s)) if s == "-") || exp_toks.len() >= 2;
        if !signed_or_wide || !toks.get(first..i)?.iter().any(|t| matches!(t, Tok::Digit(_))) {
            return None;
        }
        let min_exp = exp_toks.iter().filter(|t| **t == Tok::Digit('0')).count();
        Some((i, marker.clone(), always_sign, min_exp))
    });
    if let Some((idx, marker, always_sign, min_exp)) = sci {
        let mant_toks = toks.get(first..idx).unwrap_or(&[]);
        let dot = mant_toks.iter().position(|t| *t == Tok::Dot);
        let (int_part, frac_part) = match dot {
            Some(d) => (mant_toks.get(..d).unwrap_or(&[]), mant_toks.get(d + 1..).unwrap_or(&[])),
            None => (mant_toks, &[][..]),
        };
        let min_int = int_part.iter().filter(|t| **t == Tok::Digit('0')).count();
        let frac_digits: Vec<char> = frac_part.iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect();
        let max_frac = frac_digits.len();
        let min_frac = frac_digits.iter().filter(|c| **c == '0').count();
        let mut mant = v * if percent { 100.0 } else { 1.0 };
        if !mant.is_finite() {
            mant = 0.0;
        }
        let mut exponent: i32 = 0;
        if mant != 0.0 {
            exponent = mant.log10().floor() as i32;
            mant /= 10f64.powi(exponent);
            // Floating-point guard: division should land in [1, 10), but nudge on error.
            for _ in 0..4 {
                if mant >= 10.0 {
                    mant /= 10.0;
                    exponent += 1;
                } else if mant < 1.0 {
                    mant *= 10.0;
                    exponent -= 1;
                } else {
                    break;
                }
            }
        }
        let mut s = format!("{mant:.max_frac$}");
        // Rounding can push 9.99… up to 10.00…: renormalise to 1.00… with exponent + 1.
        if s.split('.').next() == Some("10") {
            exponent += 1;
            s = format!("{:.max_frac$}", 1.0f64);
        }
        let (ip, fp) = s.split_once('.').unwrap_or((s.as_str(), ""));
        let mut fp = fp.to_string();
        while fp.len() > min_frac && fp.ends_with('0') {
            fp.pop();
        }
        let ip = ip.trim_start_matches('0');
        let ip = if ip.is_empty() && min_int == 0 {
            String::new()
        } else if ip.len() < min_int {
            format!("{}{ip}", "0".repeat(min_int - ip.len()))
        } else {
            ip.to_string()
        };
        let mut num = ip;
        if !fp.is_empty() {
            num.push(dec);
            num.push_str(&fp);
        }
        if num.is_empty() {
            num.push('0');
        }
        let exp_sign = if exponent < 0 {
            "-"
        } else if always_sign {
            "+"
        } else {
            ""
        };
        let exp_abs = exponent.unsigned_abs().to_string();
        let exp_str = if exp_abs.len() < min_exp { format!("{}{exp_abs}", "0".repeat(min_exp - exp_abs.len())) } else { exp_abs };
        let before = lits(toks.get(..first).unwrap_or(&[]));
        let after = lits(toks.get(last + 1..).unwrap_or(&[]));
        let out = format!("{before}{num}{marker}{exp_sign}{exp_str}{after}");
        let out = out.trim();
        return if minus && !out.is_empty() { format!("-{out}") } else { out.to_string() };
    }
    let number = toks.get(first..=last).unwrap_or(&[]);
    let dot = number.iter().position(|t| *t == Tok::Dot);
    let (int_part, frac_part) = match dot {
        Some(d) => (number.get(..d).unwrap_or(&[]), number.get(d + 1..).unwrap_or(&[])),
        None => (number, &[][..]),
    };
    // Grouping: a comma (or the locale's space, written literally) between integer digits.
    let grouping = int_part.iter().any(|t| *t == Tok::Comma || matches!(t, Tok::Lit(s) if s.trim().is_empty()));
    // Commas right after the last digit scale by a thousand each (`#,##0,` = thousands).
    let after = toks.get(last + 1..).unwrap_or(&[]);
    let scale_commas = if dot.is_none() { after.iter().take_while(|t| **t == Tok::Comma).count() } else { 0 };
    let min_int = int_part.iter().filter(|t| **t == Tok::Digit('0')).count();
    let frac_digits: Vec<char> = frac_part.iter().filter_map(|t| if let Tok::Digit(c) = t { Some(*c) } else { None }).collect();
    let max_frac = frac_digits.len();
    let min_frac = frac_digits.iter().filter(|c| **c == '0').count();
    let mut x = v * if percent { 100.0 } else { 1.0 } / 1000f64.powi(scale_commas as i32);
    if !x.is_finite() {
        x = 0.0;
    }
    let s = format!("{x:.max_frac$}");
    let (ip, fp) = s.split_once('.').unwrap_or((s.as_str(), ""));
    let mut fp = fp.to_string();
    while fp.len() > min_frac && fp.ends_with('0') {
        fp.pop();
    }
    let ip = ip.trim_start_matches('0');
    let ip = if ip.len() < min_int { format!("{}{ip}", "0".repeat(min_int - ip.len())) } else { ip.to_string() };
    let mut num = if grouping { group(&ip, sep) } else { ip };
    if !fp.is_empty() {
        num.push(dec);
        num.push_str(&fp);
    }
    let lits = |ts: &[Tok]| -> String {
        ts.iter()
            .filter_map(|t| match t {
                Tok::Lit(s) => Some(s.clone()),
                Tok::Percent => Some("%".into()),
                _ => None,
            })
            .collect()
    };
    let before = lits(toks.get(..first).unwrap_or(&[]));
    let after = lits(toks.get(last + 1..).unwrap_or(&[]));
    let out = format!("{before}{num}{after}");
    let out = out.trim();
    if minus && !out.is_empty() { format!("-{out}") } else { out.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUB: &str = r#"_-* #\ ##0\ "₽"_-;\-* #\ ##0\ "₽"_-;_-* "-"??\ "₽"_-;_-@_-"#;

    #[test]
    fn russian_accounting_roubles() {
        assert_eq!(format(RUB, 800_000.0, Some("ru-RU")), "800\u{a0}000 ₽");
        assert_eq!(format(RUB, 1_234_567.0, Some("ru-RU")), "1\u{a0}234\u{a0}567 ₽");
        assert_eq!(format(RUB, -5_000.0, Some("ru-RU")), "-5\u{a0}000 ₽");
        assert_eq!(format(RUB, 0.0, Some("ru-RU")), "- ₽");
    }

    #[test]
    fn common_codes() {
        assert_eq!(format("#,##0", 7000.0, Some("ru-RU")), "7\u{a0}000");
        assert_eq!(format("#,##0", 7000.0, Some("en-US")), "7,000");
        assert_eq!(format("#,##0", 7000.0, Some("de-DE")), "7.000");
        assert_eq!(format("0.0%", 0.125, Some("ru-RU")), "12,5%");
        assert_eq!(format("0.0%", 0.125, None), "12.5%");
        assert_eq!(format("0%", 0.0, None), "0%");
        assert_eq!(format("General", 2.5, Some("ru-RU")), "2,5");
        assert_eq!(format("General", 400_000.0, Some("ru-RU")), "400000");
        assert_eq!(format("0.00", 1.23456, None), "1.23");
        assert_eq!(format("#,##0.0#", 1234.5, None), "1,234.5");
        assert_eq!(format("#,##0,", 12_345_678.0, None), "12,346");
        assert_eq!(format("[$₽-419]#,##0", 1500.0, Some("ru-RU")), "₽1\u{a0}500");
        assert_eq!(format("0;[Red]-0", -3.0, None), "-3");
        assert_eq!(format("0;0", -3.0, None), "3");
        assert_eq!(format("0", -3.0, None), "-3");
        assert_eq!(format("\"x\"0", 5.0, None), "x5");
    }

    #[test]
    fn scientific_codes() {
        // Regression for the audit issue: exponent placeholders were folded into
        // fractional digits, so 1234 with `0.00E+00` came out as "1234.0000".
        assert_eq!(format("0.00E+00", 1234.0, None), "1.23E+03");
        assert_eq!(format("0.00E+00", 0.0, None), "0.00E+00");
        assert_eq!(format("0.00E+00", -0.00123, None), "-1.23E-03");
        assert_eq!(format("0.00E+00", 12_345.0, None), "1.23E+04");
        assert_eq!(format("0E+00", 1234.0, None), "1E+03");
        // `E-` shows a sign only for negative exponents; the width pads with zeros.
        assert_eq!(format("0.00E-00", 1234.0, None), "1.23E03");
        assert_eq!(format("0.00E+000", 1234.0, None), "1.23E+003");
        // Rounding the mantissa up to 10 renormalises instead of printing "10.00".
        assert_eq!(format("0.00E+00", 9999.0, None), "1.00E+04");
        // Locale decimal mark applies to the mantissa, not the exponent.
        assert_eq!(format("0.00E+00", 1234.0, Some("de-DE")), "1,23E+03");
    }

    #[test]
    fn never_panics() {
        for code in ["", ";", ";;;", "\"", "[", "\\", "0.", ".", "#,", "%", "_", "*", "@", "0E+00", "[$", "\"unterminated"] {
            for v in [0.0, -1.5, 1e300, f64::NAN, f64::INFINITY, 1e-300] {
                let _ = format(code, v, Some("ru"));
            }
        }
    }
}
