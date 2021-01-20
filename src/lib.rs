//! The [`CostOf.Life`] calculator library.
//!
//! Provides functions to calculate the per diem cost
//! of an expense over a time range.
//!
//! [`CostOf.Life`]: http://thecostof.life

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate};
use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
pub use slug::slugify;
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt;
use std::str::FromStr;
pub use uuid::Uuid;

mod utils;

// Let's use generic errors
type Result<T> = std::result::Result<T, ValisError>;

#[derive(Debug, Clone)]
pub enum ValisError {
    InvalidLifetimeFormat(String),
    InvalidDateFormat(String),
    InvalidAmount(String),
    GenericError(String),
    InputError(String),
    Unauthorized,
}

impl fmt::Display for ValisError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<uuid::Error> for ValisError {
    fn from(error: uuid::Error) -> Self {
        Self::InputError(error.to_string())
    }
}

impl Error for ValisError {}

// initialize regexp
lazy_static! {
    static ref RE_TIMEWINDOW: Regex = Regex::new(r"(([1-9]{1}[0-9]*)([dwmy]))").unwrap();
}

fn extract_timewindow(text: &str) -> (&str, i64) {
    match RE_TIMEWINDOW.captures(text) {
        Some(c) => (
            c.get(3).map_or("d", |unit| unit.as_str()),
            c.get(2).map_or(1, |a| a.as_str().parse::<i64>().unwrap()),
        ),
        None => ("d", 1),
    }
}

/// A time range with duration and repetition
///
#[derive(Debug, Clone)]
pub enum TimeWindow {
    UpTo,
    SingleDay,
    Year(i64),
    Month(u32),
    Week(i64),
    Day(i64),
}

impl TimeWindow {
    /// Returns the number of days from a given date.
    ///
    /// This is significant con calculate the exact amount
    /// of days considering months and leap years
    pub fn get_days_since(&self, since: &NaiveDate) -> i64 {
        match self {
            Self::Month(amount) => {
                // compute the total number of months (nm)
                let nm = since.month() + amount;
                // match nm (number of months) and calculate the end year / month
                let (y, m) = (since.year() as u32 + nm / 12, nm % 12);
                // wrap the result with the correct type
                let (y, m, d) = (y as i32, m, since.day());
                // calculate the end date
                let end = NaiveDate::from_ymd(y, m, d);
                // count the days
                end.signed_duration_since(*since).num_days()
            }
            Self::Year(amount) => {
                let ny = since.year() + *amount as i32;
                let end = NaiveDate::from_ymd(ny, since.month(), since.day());
                // count the days
                end.signed_duration_since(*since).num_days()
            }
            Self::Week(amount) => amount * 7,
            Self::Day(amount) => *amount,
            Self::SingleDay => 1,
            Self::UpTo => 0,
        }
    }

    /// Range returns the date range from a date adding the time window
    ///
    pub fn range(&self, since: &NaiveDate) -> (NaiveDate, NaiveDate) {
        let since = since.clone();
        match self {
            Self::UpTo => (utils::date(1, 1, 0000), since),
            _ => (since, since + Duration::days(self.get_days_since(&since))),
        }
    }

    // End date returns the exact date when the time window will end (inclusive)
    pub fn end_date(&self, since: &NaiveDate) -> NaiveDate {
        (*since + Duration::days(self.get_days_since(since))).pred()
    }

    pub fn offset(&self, from: &NaiveDate) -> NaiveDate {
        self.range(from).1
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
            Self::Year(amount) => 365.25 * (*amount) as f64,
            Self::Month(amount) => 30.44 * (*amount) as f64,
            Self::Week(amount) => 7.0 * (*amount) as f64,
            Self::Day(amount) => (*amount) as f64,
            Self::SingleDay => 1.0,
            Self::UpTo => 0.0,
        }
    }
}

impl FromStr for TimeWindow {
    type Err = ValisError;

    fn from_str(s: &str) -> Result<TimeWindow> {
        let (period, amount) = extract_timewindow(s);
        match period {
            "w" => Ok(TimeWindow::Week(amount)),
            "y" => Ok(TimeWindow::Year(amount)),
            "m" => Ok(TimeWindow::Month(amount as u32)),
            _ => Ok(TimeWindow::Day(amount)),
        }
    }
}

impl PartialEq for TimeWindow {
    fn eq(&self, other: &Self) -> bool {
        self.get_days_approx() == other.get_days_approx()
    }
}

impl fmt::Display for TimeWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Year(amount) => write!(f, "{}y", amount),
            Self::Month(amount) => write!(f, "{}m", amount),
            Self::Week(amount) => write!(f, "{}w", amount),
            Self::Day(amount) => write!(f, "{}d", amount),
            Self::SingleDay => write!(f, "1d"),
            Self::UpTo => write!(f, "0d"),
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
impl RelState {
    pub fn emoji(&self) -> String {
        match self {
            Self::Root => "☀️".to_owned(),
            Self::Active(_, _) => "🟢".to_owned(),
            Self::Passive(_, _) => "⚪".to_owned(),
            Self::Former(_, _) => "⚫".to_owned(),
            Self::Disabled(_, _) => "-".to_owned(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Tag {
    Generic(String), // simple tag
    Feature(String), // wood, web design, sale, rust,
    Group(String),   // family, friends, colleague, employee, customer
    Link(String),    // various urls if relevant
    // contextual roles
    Role(String), // this is a role within the main context
    // system
    System(String),
}

impl Tag {
    pub fn from(prefix: &str, label: &str) -> Self {
        Self::from_str(&format!("{}:{}", prefix, label)).unwrap()
    }

    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Feature(_) => "feat",
            Self::Group(_) => "group",
            Self::Link(_) => "link",
            Self::Generic(_) => "tag",
            Self::Role(_) => "role",
            Self::System(_) => "sys",
        }
    }

    pub fn slug(&self) -> String {
        slugify(self.to_string())
    }

    pub fn to_string_full(&self) -> String {
        format!("{}:{}", self.prefix(), self.to_string())
    }
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
            Some(("sys", v)) => Ok(Tag::System(v.to_string())),
            Some(("tag", v)) => Ok(Tag::Generic(v.to_string())),
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
            Self::Feature(label) => write!(f, "{}", label),
            Self::Group(label) => write!(f, "{}", label),
            Self::Link(label) => write!(f, "{}", label),
            Self::Generic(label) => write!(f, "{}", label),
            Self::Role(label) => write!(f, "{}", label),
            Self::System(label) => write!(f, "{}", label),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
            Self::Limited(tag) => write!(f, "{}:{}", tag.prefix(), tag.slug()),
        }
    }
}

impl FromStr for ACL {
    type Err = ValisError;

    fn from_str(s: &str) -> Result<ACL> {
        match s {
            "" | "public" => Ok(Self::Public),
            "sponsor" => Ok(Self::Sponsor),
            any => match utils::split_once(any, ':') {
                Some((p, l)) => Ok(Self::Limited(Tag::from(p, l))),
                None => Err(ValisError::InputError("cannot read acl".to_string())),
            },
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum EventType {
    Log(String),
    Action(String, String, usize),
}

impl EventType {
    pub fn is_log(&self) -> bool {
        match self {
            Self::Log(_) => true,
            _ => false,
        }
    }
}

/// The Actor is a participant of an event
///
/// The Lead is the one triggering the action
/// The Starring are entities mentioned of an action
/// The Background are entities object of te action
///
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Actor {
    RecordedBy(Uuid), // notes, meeting transcript, etc
    Subject(Uuid),    // notes about something/someone
    Lead(Uuid),       // meetings, events, etc
    Starring(Uuid),   // entities attending
    Background(Uuid), // context
}

impl Actor {
    pub fn from_str(input: &str) -> Result<Actor> {
        match utils::split_once(input, ':') {
            Some((p, v)) => Self::from(p, v),
            _ => Err(ValisError::InputError("unrecognized".to_string())),
        }
    }

    pub fn from(prefix: &str, uid: &str) -> Result<Actor> {
        match prefix {
            "auth" => Ok(Actor::RecordedBy(Uuid::from_str(uid)?)),
            "lead" => Ok(Actor::Lead(Uuid::from_str(uid)?)),
            "star" => Ok(Actor::Starring(Uuid::from_str(uid)?)),
            "back" => Ok(Actor::Background(Uuid::from_str(uid)?)),
            "subj" => Ok(Actor::Subject(Uuid::from_str(uid)?)),
            _ => Err(ValisError::InputError("unrecognized".to_string())),
        }
    }

    pub fn uid(&self) -> String {
        match self {
            Self::Lead(uid) => utils::id(uid),
            Self::Starring(uid) => utils::id(uid),
            Self::Background(uid) => utils::id(uid),
            Self::RecordedBy(uid) => utils::id(uid),
            Self::Subject(uid) => utils::id(uid),
        }
    }
}
impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RecordedBy(uid) => write!(f, "author:{}", utils::id(uid)),
            Self::Lead(uid) => write!(f, "lead:{}", utils::id(uid)),
            Self::Starring(uid) => write!(f, "star:{}", utils::id(uid)),
            Self::Background(uid) => write!(f, "back:{}", utils::id(uid)),
            Self::Subject(uid) => write!(f, "subj:{}", utils::id(uid)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Event {
    pub uid: Uuid,
    pub recorded_at: DateTime<FixedOffset>,
    pub kind: EventType,
    pub content: Option<String>,
    // Entities
    pub actors: Vec<Actor>,
    // ACL
    visibility: Vec<ACL>,
}

impl Event {
    pub fn new() -> Event {
        Event {
            uid: Uuid::new_v4(),
            recorded_at: utils::now_local(),
            kind: EventType::Action("raw".to_string(), "msg".to_string(), 1),
            content: None,
            actors: vec![Actor::Lead(Uuid::new_v4())],
            visibility: vec![],
        }
    }

    pub fn log(title: &str, subject: &Entity, msg: Option<String>) -> Event {
        Event {
            uid: Uuid::new_v4(),
            recorded_at: utils::now_local(),
            kind: EventType::Log(title.to_owned()),
            content: msg,
            actors: vec![Actor::Lead(subject.uid)],
            visibility: vec![],
        }
    }

    pub fn action(
        source: &str,
        name: &str,
        weight: usize,
        content: Option<String>,
        actors: &[Actor],
    ) -> Event {
        Event {
            uid: Uuid::new_v4(),
            recorded_at: utils::now_local(),
            kind: EventType::Action(source.to_owned(), name.to_owned(), weight),
            content: content,
            actors: actors.to_owned(),
            visibility: vec![],
        }
    }

    pub fn uid(&self) -> String {
        utils::id(&self.uid)
    }
}

/// The RelQuality describes the quality of a relationship in a moment in time.
///
/// it is bound to a thing and it's relative to the root entity
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RelQuality {
    Neutral(NaiveDate, Option<NaiveDate>),  // neutral
    Formal(NaiveDate, Option<NaiveDate>),   // businesslike
    Friendly(NaiveDate, Option<NaiveDate>), // actively friendly
    Tense(NaiveDate, Option<NaiveDate>),    // with some tension in between
    Hostile(NaiveDate, Option<NaiveDate>),  // full out hostile
}

impl RelQuality {
    pub fn emoji(&self) -> String {
        match self {
            Self::Neutral(_, _) => "😐".to_owned(),
            Self::Formal(_, _) => "👔".to_owned(),
            Self::Friendly(_, _) => "🙂".to_owned(),
            Self::Tense(_, _) => "☹️".to_owned(),
            Self::Hostile(_, _) => "😠".to_owned(),
        }
    }

    pub fn from_emoji(emoji: &str, since: NaiveDate, to: Option<NaiveDate>) -> Option<Self> {
        match emoji {
            "😐" => Some(Self::Neutral(since, to)),
            "👔" => Some(Self::Formal(since, to)),
            "🙂" => Some(Self::Friendly(since, to)),
            "☹️" => Some(Self::Tense(since, to)),
            "😠" => Some(Self::Hostile(since, to)),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RelType {
    RelatedTo,                                  // generic link
    Role(String, NaiveDate, Option<NaiveDate>), // this is the main context
    BelongsTo(NaiveDate, NaiveDate),            // this a context root
    MemberOf(NaiveDate, NaiveDate),             // indicate the context of the thing
}

impl RelType {
    pub fn get_label(&self) -> String {
        match self {
            Self::RelatedTo => "related_to".to_string(),
            Self::Role(l, _s, _u) => format!("rl:{}", l),
            Self::BelongsTo(_s, _u) => "bt".to_string(),
            Self::MemberOf(_s, _u) => "mo".to_string(),
        }
    }
}

impl fmt::Display for RelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RelatedTo => write!(f, "related_to"),
            Self::Role(l, s, u) => write!(f, ":{}:{:?}:{:?}", l, s, u),
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

impl Rel {
    pub fn new(target: &Entity) -> Rel {
        Rel {
            target: target.uid,
            kind: RelType::RelatedTo,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entity {
    pub uid: Uuid,
    pub pass: Option<String>,
    // descriptive
    pub name: String, // Ada, Kitchen Table, Google
    pub tags: HashMap<String, Tag>,
    pub description: String,
    pub handles: HashMap<String, String>, // email, telegram, phone
    // contextual data
    class: String, // person / object / company / project
    pub state: RelState,
    pub quality: RelQuality,
    pub sponsor: Uuid, // the uid of the sponsor for this thing that must be a person
    // service dates
    created_on: NaiveDate,
    pub updated_on: NaiveDate,
    // next action
    pub next_action_updated_on: NaiveDate, // last time it was updated
    pub next_action_date: NaiveDate,       // in days
    pub next_action_note: String,
    // relationships
    pub relationships: Vec<Rel>,
    // ACL
    pub visibility: Vec<ACL>,
}

/// Holds a transaction information
///
///
impl Entity {
    // Getters
    pub fn name(&self) -> &str {
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

    pub fn get_next_action_headline(&self) -> String {
        for l in self.next_action_note.split('\n') {
            if l.trim().len() > 0 {
                return l.to_string();
            }
        }
        String::new()
    }

    pub fn get_pwd_hash(&self) -> Option<String> {
        self.pass.clone()
    }

    /// Tells if the Entity as a tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains_key(&slugify(&tag))
    }

    /// actions
    pub fn action_within(&self, date: &NaiveDate) -> bool {
        self.next_action_date <= *date
    }

    /// actions
    pub fn action_within_range(&self, from: &NaiveDate, to: &NaiveDate) -> bool {
        self.next_action_date >= *from && self.next_action_date < *to
    }

    /// Get the progress of the transaction at date
    ///
    /// None will use today as a data
    pub fn get_progress(&self, d: &Option<NaiveDate>) -> f32 {
        let d = match d {
            Some(d) => *d,
            None => utils::today(),
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

    /// Update the updated_on field
    fn touch(mut self) -> Self {
        self.updated_on = utils::today();
        self
    }

    /// Set the next action date and the note
    ///
    /// It does not change the updatd
    pub fn next_action(&mut self, date: NaiveDate, note: String) {
        self.next_action_date = date;
        self.next_action_note = note;
        self.next_action_updated_on = utils::today();
    }

    pub fn with_next_action(mut self, date: NaiveDate, note: String) -> Self {
        self.next_action_date = date;
        self.next_action_note = note;
        self.next_action_updated_on = utils::today();
        self
    }

    pub fn with_handle(mut self, label: &str, id: &str) -> Self {
        self.handles.insert(label.to_owned(), id.to_owned());
        self.touch()
    }

    /// add a tag to an entity
    pub fn tag(mut self, tag: Tag) -> Self {
        self.tags.insert(slugify(&tag.to_string_full()), tag);
        self.touch()
    }

    pub fn with_sponsor(mut self, sponsor: &Entity) -> Self {
        self.sponsor = sponsor.uid.clone();
        self.touch()
    }

    pub fn self_sponsored(mut self) -> Self {
        self.sponsor = self.uid.clone();
        self.touch()
    }

    pub fn with_password(mut self, pass: Option<&String>) -> Self {
        self.pass = match pass {
            Some(p) => Some(utils::hash(p)),
            None => None,
        };
        self.touch()
    }

    pub fn change_quality(mut self, new: RelQuality) -> Self {
        if self.quality == new {
            return self;
        }
        self.quality = new;
        self.touch()
    }

    pub fn add_relation(mut self, rel: &Rel) -> Self {
        self.relationships.push(rel.to_owned());
        self
    }

    pub fn authorized(&self, pwd: Option<&String>) -> Result<()> {
        match &self.pass {
            Some(ph) => match pwd.is_some() && pwd.unwrap() == ph {
                true => Ok(()),
                false => Err(ValisError::Unauthorized),
            },
            None => match pwd.is_none() {
                true => Ok(()),
                false => Err(ValisError::Unauthorized),
            },
        }
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
        pass: Option<String>,
        tags: Vec<&str>,
        description: &str,
        handles: Vec<(&str, &str)>,
        class: &str,
        state: RelState,
        quality: RelQuality,
        sponsor: uuid::Uuid,
        created_on: NaiveDate,
        updated_on: NaiveDate,
        next_action_updated_on: NaiveDate,
        next_action_date: NaiveDate,
        next_action_note: &str,
        relationships: Vec<Rel>,
        visibility: Vec<ACL>,
    ) -> Result<Entity> {
        let tx = Entity {
            uid,
            name: name.trim().to_string(),
            pass,
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
            next_action_updated_on,
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
            None,
            vec![],
            "",
            vec![],
            class,
            RelState::Active(utils::today(), None),
            RelQuality::Neutral(utils::today(), None),
            uid,
            utils::today(),
            utils::today(),
            utils::today(),
            utils::today().succ(),
            "to update",
            vec![],
            vec![],
        )
    }

    pub fn uid(&self) -> String {
        utils::id(&self.uid)
    }
    pub fn sponsor_uid(&self) -> String {
        utils::id(&self.sponsor)
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

pub fn id(prefix: &str, value: &str) -> String {
    format!("{}:{}", prefix, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use utils::*;
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
    fn test_time_window() {
        let tests = vec![
            (("1d", today(), 1, "1d"), TimeWindow::Day(1)),
            (("10d", today(), 10, "10d"), TimeWindow::Day(10)),
            (("100d", today(), 100, "100d"), TimeWindow::Day(100)),
            (("1w", today(), 7, "1w"), TimeWindow::Week(1)),
            (("7w", today(), 49, "7w"), TimeWindow::Week(7)),
            (("10w", today(), 70, "10w"), TimeWindow::Week(10)),
            (("20y", date(1, 1, 2020), 7305, "20y"), TimeWindow::Year(20)),
            (("20y", date(1, 1, 2021), 7305, "20y"), TimeWindow::Year(20)),
            (("1y", date(1, 1, 2020), 366, "1y"), TimeWindow::Year(1)),
            (("1y", date(1, 1, 2021), 365, "1y"), TimeWindow::Year(1)),
            (("1m", date(1, 1, 2021), 31, "1m"), TimeWindow::Month(1)),
            (("12m", date(1, 1, 2021), 365, "12m"), TimeWindow::Month(12)),
            (("", today(), 1, "1d"), TimeWindow::Day(1)),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("test_parse_time_window#{}", i);

            let (lifetime_spec, lifetime_exp) = t;
            let (input_str, start_date, duration_days, to_str) = lifetime_spec;

            assert_eq!(
                input_str
                    .parse::<TimeWindow>()
                    .expect("test_parse_lifetime error"),
                *lifetime_exp,
            );
            // this make sense only with the assertion above
            assert_eq!(lifetime_exp.get_days_since(start_date), *duration_days);
            // to string
            assert_eq!(lifetime_exp.to_string(), *to_str);
        }
    }

    #[test]
    fn test_ranges() {
        let tests = vec![
            (today(), TimeWindow::Day(1), (today(), after(1)), after(1)),
            (today(), TimeWindow::Week(1), (today(), after(7)), after(7)),
            (
                date(3, 1, 2000),
                TimeWindow::Week(1),
                (date(3, 1, 2000), date(10, 1, 2000)),
                date(10, 1, 2000),
            ),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("test_ranges#{}", i);

            let (window_from, window_exp, range_exp, offset_exp) = t;
            println!(
                "{} - {}:{} -> {}",
                window_from.format("%A %d.%m"),
                range_exp.0.format("%A %d.%m"),
                range_exp.1.format("%A %d.%m"),
                offset_exp.format("%A %d.%m")
            );
            // this make sense only with the assertion above
            assert_eq!(window_exp.range(window_from), *range_exp);
            assert_eq!(window_exp.end_date(window_from), range_exp.1.pred());
            assert_eq!(window_exp.offset(window_from), *offset_exp)
        }
    }

    #[test]
    fn test_tags() {
        let tests = vec![
            (
                "skill:Design",
                Tag::Feature("Design".to_string()),
                ("design", "Design", "feat:Design"),
            ),
            (
                "cat:Books & Magazines",
                Tag::Group("Books & Magazines".to_string()),
                (
                    "books-magazines",
                    "Books & Magazines",
                    "group:Books & Magazines",
                ),
            ),
            (
                "link:https://meetvalis.com",
                Tag::Link("https://meetvalis.com".to_string()),
                (
                    "https-meetvalis-com",
                    "https://meetvalis.com",
                    "link:https://meetvalis.com",
                ),
            ),
            (
                "Good",
                Tag::Generic("Good".to_string()),
                ("good", "Good", "tag:Good"),
            ),
            (
                "tag:Better",
                Tag::Generic("Better".to_string()),
                ("better", "Better", "tag:Better"),
            ),
            (
                "role:Project Manager",
                Tag::Role("Project Manager".to_string()),
                ("project-manager", "Project Manager", "role:Project Manager"),
            ),
            (
                "sys:Admin",
                Tag::System("Admin".to_string()),
                ("admin", "Admin", "sys:Admin"),
            ),
        ];

        for (i, t) in tests.iter().enumerate() {
            println!("test_tags#{}", i);
            let (tag_in, tag_exp, tag_shapes) = t;
            let (slug, label, full) = tag_shapes;

            assert_eq!(Tag::from_str(tag_in).unwrap(), *tag_exp);
            assert_eq!(tag_exp.slug(), *slug);
            assert_eq!(tag_exp.to_string(), *label);
            assert_eq!(tag_exp.to_string_full(), *full);
        }
    }
}

#[test]
fn test_acl() {
    let tests = vec![
        (
            "group:Management",
            ACL::Limited(Tag::Group("Management".to_string())),
            (Ok(()), "group:management"),
        ),
        ("public", ACL::Public, (Ok(()), "public")),
        ("", ACL::Public, (Ok(()), "public")),
        ("sponsor", ACL::Sponsor, (Ok(()), "sponsor")),
        ("whatever", ACL::Public, (Err(()), "")),
    ];

    for (i, t) in tests.iter().enumerate() {
        println!("test_acl#{}", i);
        let (acl_in, acl_exp, acl_shapes) = t;
        let (res, label) = acl_shapes;

        let acl = ACL::from_str(acl_in);
        assert_eq!(acl.is_err(), res.is_err());
        if acl.is_err() {
            return;
        }
        assert_eq!(acl.unwrap(), *acl_exp);
        assert_eq!(acl_exp.to_string(), *label);
    }
}

#[test]
fn test_actor() {
    let tests = vec![
        (
            "group:Management",
            ACL::Limited(Tag::Group("Management".to_string())),
            (Ok(()), "group:management"),
        ),
        ("public", ACL::Public, (Ok(()), "public")),
        ("", ACL::Public, (Ok(()), "public")),
        ("sponsor", ACL::Sponsor, (Ok(()), "sponsor")),
        ("whatever", ACL::Public, (Err(()), "")),
    ];

    for (i, t) in tests.iter().enumerate() {
        println!("test_actor#{}", i);
        let (actor_in, actor_exp, actor_shapes) = t;
        let (res, label) = actor_shapes;

        let actor = ACL::from_str(actor_in);
        assert_eq!(actor.is_err(), res.is_err());
        if actor.is_err() {
            return;
        }
        assert_eq!(actor.unwrap(), *actor_exp);
        assert_eq!(actor_exp.to_string(), *label);
    }
}
