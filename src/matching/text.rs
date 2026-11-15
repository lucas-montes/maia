//! The one normaliser every matcher shares, and the purchase day a card
//! payment's description carries.
//!
//! Merchant names, statement descriptions and printed product names all go
//! through [`tokens`]: lower case, no diacritics, no punctuation, no legal
//! form (`d.o.o.`, `SAS`), no card boilerplate (`Paiement par carte X6510`)
//! and no bare numbers. What is left is what names the shop or the product.

use chrono::{Datelike, NaiveDate, TimeZone, Utc};

/// Legal forms written as one token once punctuation is gone.
const LEGAL_TOKENS: &[&str] = &[
    "doo", "dd", "kd", "jdoo", "sa", "sas", "sasu", "sarl", "eurl", "snc", "gmbh", "ag", "kg",
    "ltd", "inc", "llc", "plc", "srl", "spa", "bv", "nv", "ab", "oy", "as", "co",
];

/// Legal forms written with dots (`d.o.o.`), which split into letters.
const LEGAL_SEQUENCES: &[&[&str]] = &[
    &["j", "d", "o", "o"],
    &["d", "o", "o"],
    &["d", "d"],
    &["k", "d"],
    &["s", "a", "s"],
    &["s", "a"],
    &["s", "r", "l"],
];

/// Words a bank wraps around a card payment or a transfer.
const BOILERPLATE: &[&str] = &[
    "paiement",
    "par",
    "carte",
    "cb",
    "prlvt",
    "prelevement",
    "sepa",
    "vir",
    "virement",
    "inst",
    "emis",
    "recu",
    "vers",
    "de",
    "achat",
    "card",
    "payment",
    "purchase",
    "pos",
];

/// Folds the diacritics the receipts and statements use onto plain letters.
fn fold(c: char) -> &'static str {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'ç' | 'ć' | 'č' => "c",
        'đ' | 'ď' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ů' => "u",
        'ý' | 'ÿ' => "y",
        'š' | 'ś' => "s",
        'ž' | 'ź' | 'ż' => "z",
        'ř' => "r",
        'ł' => "l",
        'œ' => "oe",
        'æ' => "ae",
        'ß' => "ss",
        _ => "",
    }
}

/// Lower case, diacritics folded, everything that is not a letter or digit a
/// space.
pub fn simplify(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let folded = fold(c);
        if !folded.is_empty() {
            out.push_str(folded);
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c.is_alphanumeric() {
            out.push(c);
        } else {
            out.push(' ');
        }
    }
    out
}

/// A card reference like `x6510`.
fn is_card_ref(token: &str) -> bool {
    token.len() >= 4 && token.starts_with('x') && token[1..].chars().all(|c| c.is_ascii_digit())
}

/// The words that name a shop or a product, in order.
pub fn tokens(text: &str) -> Vec<String> {
    let raw: Vec<String> = simplify(text)
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    'outer: while i < raw.len() {
        for seq in LEGAL_SEQUENCES {
            if raw.len() - i >= seq.len()
                && raw[i..i + seq.len()]
                    .iter()
                    .zip(seq.iter())
                    .all(|(a, b)| a == b)
            {
                i += seq.len();
                continue 'outer;
            }
        }
        let t = &raw[i];
        i += 1;
        if LEGAL_TOKENS.contains(&t.as_str())
            || BOILERPLATE.contains(&t.as_str())
            || is_card_ref(t)
            || t.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        out.push(t.clone());
    }
    out
}

/// [`tokens`] joined with single spaces: the key aliases are stored under.
pub fn normalize(text: &str) -> String {
    tokens(text).join(" ")
}

/// How much of `a` is in `b`: the share of `a`'s tokens that `b` holds.
pub fn overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() {
        return 0.0;
    }
    let hits = a.iter().filter(|t| b.contains(t)).count();
    hits as f64 / a.len() as f64
}

/// Title Case for a name built from a receipt: `LIDL HRVATSKA` → `Lidl Hrvatska`.
pub fn title(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

/// A tax number compared as digits and letters only, upper case: `HR 660-89`
/// and `hr66089` are the same number.
pub fn tax_key(raw: &str) -> String {
    raw.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// The digits of a tax number without a leading country code: a VAT number
/// `HR66089976432` holds the OIB `66089976432`.
pub fn tax_digits(raw: &str) -> String {
    let key = tax_key(raw);
    key.trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .to_string()
}

/// Midnight UTC of a day, in epoch milliseconds.
pub fn day_start_ms(date: NaiveDate) -> i64 {
    Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap())
        .timestamp_millis()
}

/// The UTC day of an epoch-milliseconds instant.
pub fn day_of(ms: i64) -> Option<NaiveDate> {
    chrono::DateTime::from_timestamp_millis(ms).map(|d| d.date_naive())
}

/// The purchase day a card payment's description carries, `… SPAR ZAGREB
/// 30/07 - Spar` → 30 July, given when it posted. The year is the posting
/// year, or the one before when that would put the purchase after the posting
/// (bought on 30/12, posted on 2/1). `None` when the text holds no `dd/mm`.
pub fn purchase_day(description: &str, posted_ms: i64) -> Option<NaiveDate> {
    let posted = day_of(posted_ms)?;
    let bytes = description.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let mut found = None;
    for i in 0..bytes.len() {
        // dd/mm, not part of a longer number and not a dd/mm/yyyy date.
        if bytes[i] == b'/'
            && i >= 2
            && digit(i - 1)
            && digit(i - 2)
            && !(i >= 3 && digit(i - 3))
            && digit(i + 1)
            && digit(i + 2)
            && !digit(i + 3)
            && bytes.get(i + 3) != Some(&b'/')
        {
            let day: u32 = description[i - 2..i].parse().ok()?;
            let month: u32 = description[i + 1..i + 3].parse().ok()?;
            found = Some((day, month));
        }
    }
    let (day, month) = found?;
    let this_year = NaiveDate::from_ymd_opt(posted.year(), month, day)?;
    if this_year > posted + chrono::Duration::days(1) {
        NaiveDate::from_ymd_opt(posted.year() - 1, month, day)
    } else {
        Some(this_year)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(y: i32, m: u32, d: u32) -> i64 {
        day_start_ms(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }

    #[test]
    fn a_receipts_legal_name_comes_down_to_the_shop() {
        assert_eq!(normalize("LIDL HRVATSKA d.o.o. k.d."), "lidl hrvatska");
        assert_eq!(normalize("KONZUM plus d.o.o."), "konzum plus");
        assert_eq!(normalize("Carrefour Market SAS"), "carrefour market");
        assert_eq!(normalize("ČALDA"), "calda");
    }

    #[test]
    fn a_card_payment_comes_down_to_the_merchant() {
        assert_eq!(
            normalize("Paiement par carte X6510 SPAR ZAGREB 30/07 - Spar"),
            "spar zagreb spar"
        );
        assert_eq!(
            normalize("Paiement par carte X6510 LIDL HRVATSKA 0196 Z 01/08 - Lidl"),
            "lidl hrvatska z lidl"
        );
        assert_eq!(normalize("Lidl"), "lidl");
    }

    #[test]
    fn overlap_is_the_share_of_the_first_in_the_second() {
        let a = tokens("Lidl");
        let b = tokens("LIDL HRVATSKA d.o.o.");
        assert_eq!(overlap(&a, &b), 1.0);
        assert_eq!(overlap(&b, &a), 0.5);
    }

    #[test]
    fn tax_numbers_compare_on_their_characters() {
        assert_eq!(tax_key("hr 660-899"), "HR660899");
        assert_eq!(tax_digits("HR66089976432"), "66089976432");
        assert_eq!(tax_digits("66089976432"), "66089976432");
    }

    #[test]
    fn titles() {
        assert_eq!(title("lidl hrvatska"), "Lidl Hrvatska");
    }

    #[test]
    fn the_purchase_day_is_read_from_the_description() {
        let d = purchase_day(
            "Paiement par carte X6510 SPAR ZAGREB 30/07 - Spar",
            ms(2026, 8, 3),
        );
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 7, 30));
        // Bought before New Year, posted after.
        let d = purchase_day("Paiement par carte X6510 SHOP 30/12", ms(2027, 1, 2));
        assert_eq!(d, NaiveDate::from_ymd_opt(2026, 12, 30));
        // No dd/mm, a full date, or a longer number: no purchase day.
        assert_eq!(purchase_day("Lidl", ms(2026, 8, 3)), None);
        assert_eq!(purchase_day("Facture 12/07/2026", ms(2026, 8, 3)), None);
        assert_eq!(purchase_day("ref 123/456", ms(2026, 8, 3)), None);
    }
}
