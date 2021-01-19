use super::utils;
use ::valis::{Entity, RelQuality, Tag, TimeWindow};
use dialoguer::console::Term;
use dialoguer::{theme::ColorfulTheme, Confirm, Editor, Input, Password, Select};
use std::str::FromStr;
use Feat::*;
use PolarAnswer::*;

mod user;
pub use user::*;

enum Feat {
    NonEmpty,
    Empty,
}

impl Feat {
    fn to_bool(&self) -> bool {
        match self {
            Self::NonEmpty => false,
            Self::Empty => true,
        }
    }
}

pub enum PolarAnswer {
    Yes,
    No,
}

impl PolarAnswer {
    pub fn to_bool(&self) -> bool {
        match self {
            Self::Yes => true,
            Self::No => false,
        }
    }

    pub fn from_bool(v: bool) -> PolarAnswer {
        match v {
            true => Self::Yes,
            false => Self::No,
        }
    }
}

/// shortcut for Confirm
fn _c(q: &str, def: PolarAnswer) -> PolarAnswer {
    PolarAnswer::from_bool(
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(q)
            .default(def.to_bool())
            .interact()
            .unwrap(),
    )
}

/// shortcut for Input
fn _i(q: &str, empty: Feat) -> String {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt(q)
        .allow_empty(empty.to_bool())
        .interact()
        .unwrap()
}

/// shortcut for Select optional input
fn _s_opt<'a, T: ?Sized>(q: &str, opts: Vec<(&'a str, &'a T)>) -> Option<&'a T> {
    match Select::with_theme(&ColorfulTheme::default())
        .with_prompt(q)
        .items(
            &opts
                .iter()
                .map(|(l, _v)| l.to_string())
                .collect::<Vec<String>>(),
        )
        .default(0)
        .interact_on_opt(&Term::stdout())
        .unwrap()
    {
        Some(i) => Some(opts[i].1),
        _ => None,
    }
}

fn _s<'a, T: ?Sized>(q: &str, opts: Vec<(&'a str, &'a T)>) -> &'a T {
    opts[Select::with_theme(&ColorfulTheme::default())
        .with_prompt(q)
        .items(
            &opts
                .iter()
                .map(|(l, _v)| l.to_string())
                .collect::<Vec<String>>(),
        )
        .default(0)
        .interact_on(&Term::stdout())
        .unwrap()]
    .1
}

/// shortcut for editor
/// fn _s(q: &str, opts: Vec<&'static str>) -> &'static str {
fn _e(q: &str) -> String {
    Editor::new().edit(q).unwrap().unwrap()
}
pub fn confirm(question: &str, default: PolarAnswer) -> PolarAnswer {
    _c(question, default)
}

pub fn input(question: &str) -> String {
    _i(question, Feat::NonEmpty)
}

pub fn password(question: &str) -> String {
    Password::with_theme(&ColorfulTheme::default())
        .with_prompt(question)
        .allow_empty_password(false)
        .interact()
        .unwrap()
}

pub fn principal_entity() -> Entity {
    let name = _i("what's your name?", Feat::NonEmpty);
    // ask if they want a password
    let pass = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("now choose a password?")
        .allow_empty_password(false)
        .with_confirmation("repeat the password", "password doesn't match!")
        .interact_on(&Term::stdout())
        .ok();
    Entity::from(&name, "person")
        .unwrap()
        .with_password(pass.as_ref())
}

pub fn root_entity() -> Entity {
    let class = _s(
        "What context do you want to manage",
        vec![
            ("Business", "org"),
            ("Personal", "private"),
            ("Both", "general"),
        ],
    );
    let name = _i(
        "How would you call the main center of your interest",
        Feat::NonEmpty,
    );
    Entity::from(&name, class).unwrap()
}

pub fn new_entity() -> Entity {
    println!("let's add a new entity");
    // get the name
    let name = _i("how shall we call it?", NonEmpty);
    // get the class
    let class = _s(
        "how will describe that",
        vec![
            ("Person", "person"),
            ("Organization", "org"),
            ("Project", "project"),
            ("Thing", "thing"),
        ],
    );
    // we have enough to create the entity
    let mut e = Entity::from(&name, class).unwrap();
    // action
    let rtw = utils::random_timewindow(1, 12, Some('w'));
    let tw = _s(
        &format!("when shall you be reminded about {}", name),
        vec![
            ("Tomorrow", "1d"),
            ("In 10 days", "10d"),
            ("In one month", "1m"),
            ("In three months", "3m"),
            ("Later", &rtw),
        ],
    );

    let nad = TimeWindow::from_str(&tw).unwrap().offset(&utils::today());
    let nan = _e("leave a note for the reminder");
    e.next_action(nad, nan);

    println!(
        "I'll remind you on {} about {} with:\n{}",
        e.next_action_date, name, e.next_action_note
    );
    // info
    if let Yes = _c("would you like to add some details?", Yes) {
        e.description = _e(&format!("write a note about {}", name));

        while let Yes = _c("add a tag?", Yes) {
            let tags = vec![
                ("Tag", "generic"),
                ("Category", "category"),
                ("Skill", "feat"),
                ("Link", "link"),
                ("Role", "role"),
            ];
            let prefix = _s("tag type", tags);
            let label = _i("what is the tag label", Feat::NonEmpty);
            e = e.tag(Tag::from(&prefix, &label));
        }
    };

    // return
    e
}

pub fn edit_entities(items: &[Entity]) -> Option<&Entity> {
    let opts = items.iter().map(|e| (e.name(), e)).collect();
    _s_opt("Which one", opts)
}

pub fn edit_entity(mut target: Entity) -> Entity {
    // action
    let rtw = utils::random_timewindow(1, 12, Some('w'));
    let tw = _s(
        &"when shall you be reminded about it",
        vec![
            ("Tomorrow", "1d"),
            ("In 10 days", "10d"),
            ("In one month", "1m"),
            ("In three months", "3m"),
            ("Later", &rtw),
        ],
    );

    let nad = TimeWindow::from_str(&tw).unwrap().offset(&utils::today());
    let nan = _e(&target.next_action_note);
    target.next_action(nad, nan);
    // ask for the quality
    let q = _s(
        &format!("how is the relationship status? {}", target.quality.emoji()),
        vec![
            ("Unchanged", "none"),
            ("Neutral", "😐"),
            ("Formal", "👔"),
            ("Friendly", "🙂"),
            ("Tense", "☹️"),
            ("Hostile", "😠"),
        ],
    );
    match RelQuality::from_emoji(q, utils::today(), None) {
        Some(q) => target.change_quality(q),
        _ => {}
    };
    // type
    target
}
