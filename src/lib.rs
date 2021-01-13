//! The [`CostOf.Life`] calculator library.
//!
//! Provides functions to calculate the per diem cost
//! of an expense over a time range.
//!
//! [`CostOf.Life`]: http://thecostof.life

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use slug::slugify;
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

mod utils;
pub use utils::*;

// Let's use generic errors
type Result<T> = std::result::Result<T, ValisError>;

#[derive(Debug, Clone)]
pub enum ValisError {
    InvalidLifetimeFormat(String),
    InvalidDateFormat(String),
    InvalidAmount(String),
    GenericError(String),
}

impl fmt::Display for ValisError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Error for ValisError {}

// initialize regexp
lazy_static! {
    static ref RE_CURRENCY: Regex = Regex::new(r"(\d+(\.\d{2})?)\p{Currency_Symbol}").unwrap();
    static ref RE_HASHTAG: Regex = Regex::new(r"^[#\.]([a-zA-Z][0-9a-zA-Z_-]*)$").unwrap();
    static ref RE_LIFETIME: Regex =
        Regex::new(r"(([1-9]{1}[0-9]*)([dwmy]))(([1-9]{1}[0-9]*)x)?").unwrap();
    static ref RE_DATE: Regex = Regex::new(r"([0-3][0-9][0-1][0-9][1-9][0-9])").unwrap();
}

fn extract_amount(input: &str) -> Option<&str> {
    RE_CURRENCY
        .captures(input)
        .and_then(|c| c.get(1).map(|m| m.as_str()))
}

fn extract_hashtag(text: &str) -> Option<&str> {
    RE_HASHTAG
        .captures(text)
        .and_then(|c| c.get(1).map(|m| m.as_str()))
}

fn extract_date(text: &str) -> Option<NaiveDate> {
    let ds = RE_DATE
        .captures(text)
        .and_then(|c| c.get(1).map(|m| m.as_str()));
    match ds {
        Some(d) => date_from_str(d),
        None => Some(today()),
    }
}

fn extract_lifetime(text: &str) -> (&str, i64, i64) {
    match RE_LIFETIME.captures(text) {
        Some(c) => (
            c.get(3).map_or("d", |unit| unit.as_str()),
            c.get(2).map_or(1, |a| a.as_str().parse::<i64>().unwrap()),
            c.get(5).map_or(1, |r| r.as_str().parse::<i64>().unwrap()),
        ),
        None => ("d", 1, 1),
    }
}

/// A time range with duration and repetition
///
#[derive(Debug, Clone)]
pub enum Lifetime {
    // amount, times
    SingleDay,
    Year { amount: i64, times: i64 },
    Month { amount: i64, times: i64 },
    Week { amount: i64, times: i64 },
    Day { amount: i64, times: i64 },
}

impl Lifetime {
    /// Returns the number of days from a given date.
    ///
    /// This is significant con calculate the exact amount
    /// of days considering months and leap years
    pub fn get_days_since(&self, since: &NaiveDate) -> i64 {
        match self {
            Self::Month { amount, times } => {
                // compute the total number of months (nm)
                let nm = since.month() + (times * amount) as u32;
                // match nm (number of months) and calculate the end year / month
                let (y, m) = (since.year() as u32 + nm / 12, nm % 12);
                // wrap the result with the correct type
                let (y, m, d) = (y as i32, m, since.day());
                // calculate the end date
                let end = NaiveDate::from_ymd(y, m, d);
                // count the days
                end.signed_duration_since(*since).num_days()
            }
            Self::Year { amount, times } => {
                let ny = since.year() + (times * amount) as i32;
                let end = NaiveDate::from_ymd(ny, since.month(), since.day());
                // count the days
                end.signed_duration_since(*since).num_days()
            }
            Self::Week { amount, times } => amount * 7 * times,
            Self::Day { amount, times } => amount * times,
            Self::SingleDay => 1,
        }
    }

    /// Approximates the size of the lifetime
    ///
    /// this function differs from the `get_days_since` by the
    /// fact that the size of months and years is approximated:
    /// - A year is 365.25 days
    /// - A month is 30.44 days
    ///
    fn get_days_approx(&self) -> f64 {
        match self {
            Self::Year { amount, times } => 365.25 * (amount * times) as f64,
            Self::Month { amount, times } => 30.44 * (amount * times) as f64,
            Self::Week { amount, times } => 7.0 * (amount * times) as f64,
            Self::Day { amount, times } => (amount * times) as f64,
            Self::SingleDay => 1.0,
        }
    }

    /// Get the number of duration repeats for the current lifetime
    ///
    ///
    pub fn get_repeats(&self) -> i64 {
        match self {
            Self::Year { times, .. } => *times,
            Self::Week { times, .. } => *times,
            Self::Day { times, .. } => *times,
            Self::Month { times, .. } => *times,
            Self::SingleDay => 1,
        }
    }
}

impl FromStr for Lifetime {
    type Err = ValisError;

    fn from_str(s: &str) -> Result<Lifetime> {
        let (period, amount, times) = extract_lifetime(s);
        match period {
            "w" => Ok(Lifetime::Week { amount, times }),
            "y" => Ok(Lifetime::Year { amount, times }),
            "m" => Ok(Lifetime::Month { amount, times }),
            _ => Ok(Lifetime::Day { amount, times }),
        }
    }
}

impl PartialEq for Lifetime {
    fn eq(&self, other: &Self) -> bool {
        self.get_days_approx() == other.get_days_approx()
    }
}

impl fmt::Display for Lifetime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Year { amount, times } => write!(f, "{}y{}x", amount, times),
            Self::Month { amount, times } => write!(f, "{}m{}x", amount, times),
            Self::Week { amount, times } => write!(f, "{}w{}x", amount, times),
            Self::Day { amount, times } => write!(f, "{}d{}x", amount, times),
            Self::SingleDay => write!(f, "1d1x"),
        }
    }
}

/// The relation state describes which kind of relationship exists from the
/// context within valis operates and the thing holding the property
///
/// This is a not explicit relation between the context and the Entity
///
/// Possible relation state are
/// - Active : the thing is a active in the context
/// - Passive: the thing is not directly engaged in a context but somehow still present
/// - Former: there isn't a connection anymore, with a date indicating when the connection was broken
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RelState {
    Root, // this would be the center of the application
    Active(NaiveDate, Option<NaiveDate>),
    Passive(NaiveDate, Option<NaiveDate>),
    Former(NaiveDate, Option<NaiveDate>),
    Disabled(NaiveDate, Option<NaiveDate>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Tag {
    Generic(String), // simple tag
    Feature(String), // wood, web design, sale, rust,
    Group(String),   // family, friends, colleague, employee, customer
    Link(String),    // various urls if relevant
    // contextual roles
    Role(String), // this is a role within the main context
}

impl FromStr for Tag {
    type Err = ValisError;

    fn from_str(s: &str) -> Result<Tag> {
        // match s.split_once(':') {
        match utils::split_once(s, ':') {
            //
            Some(("feat", v)) => Ok(Tag::Feature(v.to_string())),
            Some(("skill", v)) => Ok(Tag::Feature(v.to_string())),
            // group
            Some(("group", v)) => Ok(Tag::Group(v.to_string())),
            Some(("category", v)) => Ok(Tag::Group(v.to_string())),
            Some(("cat", v)) => Ok(Tag::Group(v.to_string())),
            // links
            Some(("link", v)) | Some(("url", v)) => Ok(Tag::Link(v.to_string())),
            // context
            Some(("role", v)) => Ok(Tag::Role(v.to_string())),
            Some(("ctx role", v)) => Ok(Tag::Role(v.to_string())),
            Some(("ext role", v)) => Ok(Tag::Role(v.to_string())),
            _ => Ok(Tag::Generic(s.to_string())),
        }
    }
}

impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        slugify(self.to_string()) == slugify(other.to_string())
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Feature(label) => write!(f, "feat:{}", label),
            Self::Group(label) => write!(f, "group:{}", label),
            Self::Link(label) => write!(f, "link:{}", label),
            Self::Generic(label) => write!(f, "{}", label),
            Self::Role(label) => write!(f, "role:{}", label),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ACL {
    Public,
    Sponsor, // message, email, webhook
    Limited(Tag),
}

impl fmt::Display for ACL {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Public => write!(f, "public"),
            Self::Sponsor => write!(f, "sponsor"),
            Self::Limited(tag) => write!(f, "{}", tag),
        }
    }
}

/// EventType describe an event
///
/// ### Log(Message)
/// Describes a system event (eg. entity created, login, logout, etc)
///
/// ### Action(Source, Message, Weight)
/// Describes an active action that triggered by an entity with an
/// associated weight.
///
/// The weight is a positive number that is associated to an event and
/// that is the used to compute the the ranking of the entities
/// based on their activity (eg. for a chat message can be the number of char written)
///  
/// The other use for the weight (with the derived metric of event frequency)
/// is to monitor entities activity to get alarms about trends.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EventType {
    Log(String),
    Action(String, String, usize),
}

/// The Actor is a participant of an event
///
/// The Lead is the one triggering the action
/// The Starring are entities mentioned of an action
/// The Background are entities object of te action
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Actor {
    Lead(Uuid),
    Starring(Uuid),
    Background(Uuid),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    recorded_at: DateTime<FixedOffset>,
    kind: EventType,
    content: Option<String>,
    // Entities
    actors: Vec<Actor>,
    // ACL
    visibility: Vec<ACL>,
}

impl Event {
    pub fn new() -> Event {
        Event {
            recorded_at: now_local(),
            kind: EventType::Action("raw".to_string(), "msg".to_string(), 1),
            content: None,
            actors: vec![Actor::Lead(Uuid::new_v4())],
            visibility: vec![],
        }
    }
}

/// The RelQuality describes the quality of a relationship in a moment in time.
///
/// it is bound to a thing and it's relative to the root entity
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RelQuality {
    Formal(NaiveDate, Option<NaiveDate>),   // businesslike
    Friendly(NaiveDate, Option<NaiveDate>), // actively friendly
    Tense(NaiveDate, Option<NaiveDate>),    // with some tension in between
    Hostile(NaiveDate, Option<NaiveDate>),  // full out hostile
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum RelType {
    Role(String, NaiveDate, Option<NaiveDate>), // this is the main context
    BelongsTo(NaiveDate, NaiveDate),            // this a context root
    MemberOf(NaiveDate, NaiveDate),             // indicate the context of the thing
}

impl RelType {
    pub fn get_label(&self) -> String {
        match self {
            Self::Role(l, _s, _u) => format!("rl:{}", l),
            Self::BelongsTo(_s, _u) => "bt".to_string(),
            Self::MemberOf(_s, _u) => "mo".to_string(),
        }
    }
}

impl PartialEq for RelType {
    fn eq(&self, other: &Self) -> bool {
        slugify(self.to_string()) == slugify(other.to_string())
    }
}

impl fmt::Display for RelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Role(l, s, u) => write!(f, "rl:{}:{:?}:{:?}", l, s, u),
            Self::BelongsTo(s, u) => write!(f, "bt:{:?}:{:?}", s, u),
            Self::MemberOf(s, u) => write!(f, "mo:{:?}:{:?}", s, u),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Rel {
    pub kind: RelType,
    pub target: Uuid,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entity {
    pub uid: Uuid,
    // descriptive
    pub name: String, // Ada, Kitchen Table, Google
    pub tags: HashMap<String, Tag>,
    pub description: String,
    handles: HashMap<String, String>, // email, telegram, phone
    // contextual data
    class: String, // person / object / company / project
    state: RelState,
    quality: RelQuality,
    sponsor: Uuid, // the uid of the sponsor for this thing that must be a person
    // service dates
    created_on: NaiveDate,
    updated_on: NaiveDate,
    // next action
    pub next_action_date: NaiveDate, // in days
    next_action_note: String,
    // relationships
    pub relationships: Vec<Rel>,
    // ACL
    pub visibility: Vec<ACL>,
}

/// Holds a transaction information
///
///
impl Entity {
    pub fn bin(&self) {}

    // Getters
    pub fn get_name(&self) -> &str {
        &self.name[..]
    }
    /// Get the tags for the tx, sorted alphabetically
    pub fn get_tags(&self) -> Vec<String> {
        self.tags
            .values()
            .map(|t| t.to_string())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect()
    }

    /// Tells if the Entity as a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains_key(&slugify(&tag))
    }

    /// Get the progress of the transaction at date
    ///
    /// None will use today as a data
    pub fn get_progress(&self, d: &Option<NaiveDate>) -> f32 {
        let d = match d {
            Some(d) => *d,
            None => today(),
        };
        // get the time range
        let (start, end) = (self.updated_on, self.next_action_date);
        if d <= start {
            // if the tx period has not started
            return 0.0;
        }
        if d >= end {
            // tx period has expired
            return 1.0;
        }
        // total number of days
        let n = (end - start).num_days() as f32;
        // number of elapsed days
        let y = (d - start).num_days() as f32;
        // duration percentage
        y / n
    }

    pub fn next_action(&mut self, date: NaiveDate, note: String) {
        self.next_action_date = date;
        self.next_action_note = note;
    }

    /// Builds a Entity using parameters
    ///
    /// # Arguments
    ///
    /// * `name` - A string slice that holds the name of the transaction
    /// * `tags` - A vector of string slices with the transaction's tags
    /// * `amount` - A string slice representing a monetary value
    /// * `starts_on` - The date of the start of the transaction
    /// * `lifetime` - The lifetime of transaction
    /// * `recorded_at` - The localized exact time when the tx was added
    /// * `src` - An option string slice with the original string used to submit the tx
    ///
    /// # Examples
    ///
    /// ```
    /// ```
    pub fn new(
        uid: uuid::Uuid,
        name: &str,
        tags: Vec<&str>,
        description: &str,
        handles: Vec<(&str, &str)>,
        class: &str,
        state: RelState,
        quality: RelQuality,
        sponsor: uuid::Uuid,
        created_on: NaiveDate,
        updated_on: NaiveDate,
        next_action_date: NaiveDate,
        next_action_note: &str,
        relationships: Vec<Rel>,
        visibility: Vec<ACL>,
    ) -> Result<Entity> {
        let tx = Entity {
            uid,
            name: name.trim().to_string(),
            tags: tags
                .iter()
                .map(|v| (slugify(v), v.parse().unwrap()))
                .collect(),
            description: description.to_string(),
            handles: handles
                .iter()
                .map(|(n, v)| (n.to_string(), v.to_string()))
                .collect(),
            class: class.to_string(),
            state,
            quality,
            sponsor,
            created_on,
            updated_on,
            next_action_date,
            next_action_note: next_action_note.to_string(),
            relationships,
            visibility,
        };
        Ok(tx)
    }

    pub fn from(name: &str, class: &str) -> Result<Entity> {
        let uid = Uuid::new_v4();
        Entity::new(
            uid,
            name,
            vec![],
            "",
            vec![],
            class,
            RelState::Active(today(), None),
            RelQuality::Formal(today(), None),
            uid,
            today(),
            today(),
            after(1),
            "to update",
            vec![],
            vec![],
        )
    }

    pub fn from_str(s: &str) -> Result<Entity> {
        // match s.split_once(':') { // until it becomes available
        match utils::split_once(s, ':') {
            Some((class, name)) => Entity::from(name.trim(), class.trim()),
            _ => Err(ValisError::GenericError(
                "cannot parse input string".to_string(),
            )),
        }
    }
}

impl FromStr for Entity {
    type Err = ValisError;
    fn from_str(s: &str) -> Result<Entity> {
        Entity::from_str(s)
    }
}

impl fmt::Display for Entity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl PartialEq for Entity {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name) && self.class.eq(&other.class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx() {
        let tests = vec![
            (
                // create by parsing
                Entity::from_str("person:Andrea "),
                (
                    Ok(()),                          // ok/error
                    "Andrea",                        // name
                    vec![("a", false)],              // tags
                    "",                              // description
                    vec![("a", false)],              // handles
                    "person",                        // class
                    RelState::Active(today(), None), // state
                    today(),                         // created_on
                    today(),                         // updated_on
                    after(1),                        // next_action_date
                    "",                              // next_action_note
                    vec![],                          // events
                ),
            ),
            (
                // create by parsing / no valid element
                Entity::from_str("bla bla"),
                (
                    Err(()),                         // ok/error
                    "Andrea",                        // name
                    vec![("a", false)],              // tags
                    "",                              // description
                    vec![("a", false)],              // handles
                    "person",                        // class
                    RelState::Active(today(), None), // state
                    today(),                         // created_on
                    today(),                         // updated_on
                    after(1),                        // next_action_date
                    "",                              // next_action_note
                    vec![Event::new()],              // events
                ),
            ),
        ];

        // run the test cases

        for (i, t) in tests.iter().enumerate() {
            println!("test_getters#{}", i);
            let (res, expected) = t;
            let (
                result,
                name,
                tags,
                description,
                handles,
                class,
                state,
                created_on,
                updated_on,
                next_action_date,
                next_action_note,
                events,
            ) = expected;
            // test for expected errors
            assert_eq!(res.is_err(), result.is_err());
            if res.is_err() {
                continue;
            }
            // test the parser
            let got = res.as_ref().unwrap();
            // test getters
            assert_eq!(got.name, name.to_string());
            assert_eq!(got.name, got.to_string());
            assert_eq!(got.created_on, *created_on);
            assert_eq!(got.updated_on, *updated_on);
            // check the tags
            tags.iter()
                .for_each(|(tag, exists)| assert_eq!(got.has_tag(tag), *exists));
            // is active
        }
    }

    #[test]
    fn test_lifetime() {
        let tests = vec![
            (
                ("1d1x", today(), 1, "1d1x"),
                Lifetime::Day {
                    amount: 1,
                    times: 1,
                },
            ),
            (
                ("10d1x", today(), 10, "10d1x"),
                Lifetime::Day {
                    amount: 10,
                    times: 1,
                },
            ),
            (
                ("10d10x", today(), 100, "10d10x"),
                Lifetime::Day {
                    amount: 10,
                    times: 10,
                },
            ),
            (
                ("1w", today(), 7, "1w1x"),
                Lifetime::Week {
                    amount: 1,
                    times: 1,
                },
            ),
            (
                ("7w", today(), 49, "7w1x"),
                Lifetime::Week {
                    amount: 7,
                    times: 1,
                },
            ),
            (
                ("10w10x", today(), 700, "10w10x"),
                Lifetime::Week {
                    amount: 10,
                    times: 10,
                },
            ),
            (
                ("20y", date(1, 1, 2020), 7305, "20y1x"),
                Lifetime::Year {
                    amount: 20,
                    times: 1,
                },
            ),
            (
                ("1y20x", date(1, 1, 2020), 7305, "1y20x"),
                Lifetime::Year {
                    amount: 1,
                    times: 20,
                },
            ),
            (
                ("20y", date(1, 1, 2021), 7305, "20y1x"),
                Lifetime::Year {
                    amount: 20,
                    times: 1,
                },
            ),
            (
                ("1y", date(1, 1, 2020), 366, "1y1x"),
                Lifetime::Year {
                    amount: 1,
                    times: 1,
                },
            ),
            (
                ("1y", date(1, 1, 2021), 365, "1y1x"),
                Lifetime::Year {
                    amount: 1,
                    times: 1,
                },
            ),
            (
                ("1m", date(1, 1, 2021), 31, "1m1x"),
                Lifetime::Month {
                    amount: 1,
                    times: 1,
                },
            ),
            (
                ("12m", date(1, 1, 2021), 365, "12m1x"),
                Lifetime::Month {
                    amount: 12,
                    times: 1,
                },
            ),
            (
                ("1m12x", date(1, 1, 2021), 365, "1m12x"),
                Lifetime::Month {
                    amount: 1,
                    times: 12,
                },
            ),
            (
                ("", today(), 1, "1d1x"),
                Lifetime::Day {
                    amount: 1,
                    times: 1,
                },
            ),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("test_parse_lifetime#{}", i);

            let (lifetime_spec, lifetime_exp) = t;
            let (input_str, start_date, duration_days, to_str) = lifetime_spec;

            assert_eq!(
                input_str
                    .parse::<Lifetime>()
                    .expect("test_parse_lifetime error"),
                *lifetime_exp,
            );
            // this make sense only with the assertion above
            assert_eq!(lifetime_exp.get_days_since(start_date), *duration_days);
            // to string
            assert_eq!(lifetime_exp.to_string(), *to_str);
        }
    }
}
