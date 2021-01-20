use chrono::{DateTime, Duration, FixedOffset, Local, NaiveDate};
use rand::Rng;

/// split  a string in two pieces
pub fn split_once(s: &str, sep: char) -> Option<(&str, &str)> {
    let x: Vec<&str> = s.splitn(2, sep).collect();
    match x.len() {
        0 | 1 => None,
        _ => Some((x[0], x[1])),
    }
}

pub fn id(uid: &uuid::Uuid) -> String {
    uid.to_simple().to_string()
}

/// Returns the current date
pub fn today() -> NaiveDate {
    Local::today().naive_utc()
}

#[cfg(test)]
pub fn after(days: i64) -> NaiveDate {
    today() + Duration::days(days)
}

/// Returns the datetime with the local timezone
pub fn now_local() -> DateTime<FixedOffset> {
    DateTime::from(Local::now())
}

pub fn random_timewindow(start: usize, limit: usize, unit: Option<char>) -> String {
    let mut rng = rand::thread_rng();
    match unit {
        Some(c) => format!("{}{}", rng.gen_range(start..limit), c),
        None => {
            let c = vec!['d', 'w', 'm', 'y'];
            format!(
                "{}{}",
                rng.gen_range(start..limit),
                c[rng.gen_range(0..c.len())]
            )
        }
    }
}

pub fn hash(data: &str) -> String {
    blake3::hash(data.as_bytes()).to_hex().to_lowercase()
}

/// Builds a date from day/month/year numeric
///
/// # Examples
///
/// ```
/// ```
pub fn date(d: u32, m: u32, y: i32) -> NaiveDate {
    NaiveDate::from_ymd(y, m, d)
}

/// Parse a date from string, it recognizes the formats
///
/// - dd/mm/yyyy
/// - dd.mm.yyyy
/// - ddmmyy
/// - dd.mm.yy
/// - dd/mm/yy
///
pub fn date_from_str(s: &str) -> Option<NaiveDate> {
    let formats = vec!["%d%m%y", "%d.%m.%y", "%d/%m/%y", "%d/%m/%Y", "%d.%m.%Y"];
    // check all the formats
    for f in formats {
        let r = NaiveDate::parse_from_str(s, f);
        if r.is_ok() {
            return Some(r.unwrap());
        }
    }
    None
}

pub fn prefix(xs: &str, ys: &str) -> String {
    // assert_eq!(xs.len(), 2);
    // assert_eq!(ys.len(), 2);
    let idx = xs
        .as_bytes()
        .iter()
        .zip(ys.as_bytes())
        .take_while(|(x, y)| x == y)
        .count();
    xs[0..idx].to_string()
}

/// Pretty print a date 
pub fn human_date(date: &NaiveDate) -> String {
    date.format("%a, %d.%m.%y").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utils() {
        let tests = vec![
            (("split:me", ':'), Some(("split", "me"))),
            (("split:me", ','), None),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("Test#{}", i);
            let (inputs, exp) = t;
            let (in_str, in_sep) = inputs;
            assert_eq!(split_once(in_str, *in_sep), *exp);
        }
        // prefix
        let tests = vec![(("2020-10-01", "2020-10-02"), "2020-10-0")];

        for (i, t) in tests.iter().enumerate() {
            println!("Test#{}", i);
            let (inputs, exp) = t;
            let (a, b) = inputs;
            assert_eq!(prefix(a, b), *exp);
        }
    }

    #[test]
    fn test_parsers() {
        // parse date
        let r = date_from_str("27/12/2020");
        assert_eq!(r.unwrap(), date(27, 12, 2020));
        // invalid date
        let r = date_from_str("30/02/2020");
        assert_eq!(r, None);
        // invalid format
        let r = date_from_str("30/02/20");
        assert_eq!(r, None);
        // dd.mm.yy
        let r = date_from_str("30.01.20");
        assert_eq!(r.unwrap(), date(30, 1, 2020));
        // dd/mm/yy
        let r = date_from_str("30/01/20");
        assert_eq!(r.unwrap(), date(30, 1, 2020));
        // dd.mm.yyyy
        let r = date_from_str("30/01/2020");
        assert_eq!(r.unwrap(), date(30, 1, 2020));
    }
}
