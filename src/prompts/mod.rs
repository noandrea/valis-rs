use ::valis::data::{
    context::ContextManager,
    ledger::DataStore,
    model::{Actor, Entity, Rel, RelQuality, Tag, TimeWindow},
    utils,
};
use chrono::NaiveDate;
use dialoguer::console::Term;
use dialoguer::{theme::ColorfulTheme, Confirm, Editor, Input, Password, Select};
use std::str::FromStr;
use Feat::*;
use PolarAnswer::*;

mod user;
pub use user::*;

pub enum Feat {
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

#[derive(PartialEq)]
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
pub fn confirm(q: &str, def: PolarAnswer) -> PolarAnswer {
    PolarAnswer::from_bool(
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(q)
            .default(def.to_bool())
            .interact_on(&Term::stdout())
            .unwrap(),
    )
}

/// shortcut for Input
pub fn input(q: &str, empty: Feat) -> String {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt(q)
        .allow_empty(empty.to_bool())
        .interact_on(&Term::stdout())
        .unwrap()
}

/// optional input return None if the input is empty
pub fn input_opt(q: &str) -> Option<String> {
    let i = input(q, Feat::Empty);
    match i.is_empty() {
        true => None,
        false => Some(i),
    }
}

/// shortcut for Select optional input
pub fn select_opt<'a, T: ?Sized>(q: &str, opts: Vec<(&'a str, &'a T)>) -> Option<&'a T> {
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

pub fn select<'a, T: ?Sized>(q: &str, opts: Vec<(&'a str, &'a T)>) -> &'a T {
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
/// fn select(q: &str, opts: Vec<&'static str>) -> &'static str {
pub fn editor(q: &str) -> Option<String> {
    Editor::new().edit(q).unwrap()
}

pub fn password(question: &str) -> String {
    Password::with_theme(&ColorfulTheme::default())
        .with_prompt(question)
        .allow_empty_password(false)
        .interact()
        .unwrap()
}

pub fn principal_entity() -> Entity {
    let name = input("what's your name?", Feat::NonEmpty);
    // ask if they want a password
    let pass = Password::with_theme(&ColorfulTheme::default())
        .with_prompt("now choose a password?")
        .allow_empty_password(false)
        .with_confirmation("repeat the password", "password doesn't match!")
        .interact_on(&Term::stdout())
        .ok();
    Entity::from(&name)
        .unwrap()
        .with_class("person")
        .with_password(pass.as_ref())
}

pub fn root_entity() -> Entity {
    let class = select(
        "What context do you want to manage",
        vec![
            ("Business", "org"),
            ("Personal", "private"),
            ("Both", "general"),
        ],
    );
    let name = input(
        "How would you call the main center of your interest",
        Feat::NonEmpty,
    );
    Entity::from(&name).unwrap().with_class(class)
}

pub fn new_entity(name: &str, sponsor: &Entity) -> Entity {
    // get the class
    let class = select(
        "how will describe that",
        vec![
            ("Person", "person"),
            ("Organization", "org"),
            ("Project", "project"),
            ("Thing", "thing"),
        ],
    );
    // we have enough to create the entity
    Entity::from(&name)
        .unwrap()
        .with_sponsor(sponsor)
        .with_class(class)
}

/// Create a new entity, but before doing so do a fuzzy search about what
/// is already in the database
pub fn new_entity_unless_exists(ds: &DataStore, name: &str, sponsor: &Entity) -> Option<Entity> {
    let existing = ds.search(name);
    if !existing.is_empty() {
        let msg = format!(
            "I've found similar entries:\n- {}\n add anyway?",
            existing
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<String>>()
                .join("\n- ")
        );
        if No == confirm(&msg, No) {
            return None;
        }
    }
    Some(new_entity(name, sponsor))
}

/// Search an entity in the datastore or ask to create a new
/// one if no result is found
///
/// Will return an Option<(Entity, bool)> where the bool indicates
/// if the entity returned is new (has been created)
pub fn select_or_create(ds: &DataStore, name: &str, sponsor: &Entity) -> Option<(Entity, bool)> {
    let res = ds.search(name);
    if res.is_empty() {
        if No == confirm("nothing found, add instead?", No) {
            return None;
        }
        return Some((new_entity(name, sponsor), true));
    }
    if let Some(r) = select_entity("please select one  (or esc/q to cancel):", &res) {
        return Some((r.clone(), false));
    }
    None
}

pub fn edit_entities(items: &[Entity]) -> Option<&Entity> {
    // TODO: messy
    let stuff: Vec<(String, usize)> = items
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let s = format!("{:40} - {}", e.name(), e.get_next_action_headline());
            (s, i)
        })
        .collect();

    match select_opt(
        "Which one",
        stuff.iter().map(|(s, i)| (&s[..], i)).collect(),
    ) {
        Some(i) => Some(&items[*i]),
        _ => None,
    }
}

/// Edit the next action date and note
fn edit_next_action(e: &mut Entity) {
    let rtw = utils::random_timewindow(1, 12, Some('w'));
    let tw = select(
        &format!("when shall you be reminded about {}", e.name()),
        vec![
            ("Today", "0d"),
            ("Tomorrow", "1d"),
            ("In 3 days", "3d"),
            ("In a week", "1w"),
            ("In two weeks", "2w"),
            ("In one month", "1m"),
            ("In three months", "3m"),
            ("In six months", "6m"),
            ("Later", &rtw),
        ],
    );

    let nad = TimeWindow::from_str(&tw).unwrap().offset(&utils::today());
    let nan = match editor("leave a note for the reminder") {
        Some(x) => x,
        None => e.next_action_note.clone(),
    };
    e.next_action(nad, nan);
}

/// Delay the next action date of an entity automatically in the future.
/// set the next_action_date to a moment in the future and the updated_on to the current date
pub fn delay_action(e: &mut Entity) {
    let rtw = utils::random_timewindow(3, 21, Some('d'));
    let nad = TimeWindow::from_str(&rtw).unwrap().offset(&utils::today());
    e.next_action_date = nad;
    e.next_action_updated_on = utils::today();
}

pub fn edit_data(ds: &mut DataStore, target: &mut Entity) {
    // info
    if let Yes = confirm("would you like to add some details?", No) {
        if let Some(desc) = editor(&format!("write a note about {}", target.name())) {
            target.description = desc;
        }
        // handles
        while let Yes = confirm("add an handle?", Yes) {
            let handles = vec![
                ("Email", "email"),
                ("Nickname", "nick"),
                ("Website", "url"),
                ("Telegram", "telegram"),
                ("LinkedIn", "linkedin"),
                ("Mobile", "mobile"),
            ];
            let prefix = select("what do you want to set", handles);
            let label = input(&format!("what is the {} handle", prefix), Feat::NonEmpty);
            target.add_handle(prefix, &label);
        }
    };

    // ask for the quality
    let prompt = format!(
        "relationship is {}, is it still the case ?",
        target.quality.emoji(),
    );
    if No == confirm(&prompt, Yes) {
        let q = select(
            "how will you describe the quality of your relationship?",
            vec![
                ("Unchanged", "none"),
                ("Neutral", "😐"),
                ("Formal", "👔"),
                ("Friendly", "🙂"),
                ("Tense", "☹️"),
                ("Hostile", "😠"),
            ],
        );
        if let Some(q) = RelQuality::from_emoji(q, utils::today(), None) {
            target.set_quality(q);
        }
    }
    // -- advanced editing
    if No == confirm("do you want to edit more details?", No) {
        println!("ok");
        return;
    }
    // relationships
    while Yes == confirm("relationships?", No) {
        if let Some(entity) = search(ds, "select target (enter to cancel)") {
            let rel = select_relationship(&entity);
            target.add_relation(&rel);
        }
    }
    // handles
    while let Yes = confirm("add an handle?", Yes) {
        let handles = vec![
            ("Email", "email"),
            ("Nickname", "nick"),
            ("Website", "url"),
            ("Telegram", "telegram"),
            ("LinkedIn", "linkedin"),
            ("Mobile", "mobile"),
            ("Github", "github"),
        ];
        let prefix = select("what do you want to set", handles);
        let label = input(&format!("what is the {} handle", prefix), Feat::NonEmpty);
        target.add_handle(prefix, &label);
    }
    //tags
    while let Yes = confirm("shall we add a tag?", No) {
        let tags = vec![
            ("Tag", "generic"),
            ("Category", "category"),
            ("Skill", "feat"),
            ("Link", "link"),
            ("Role", "role"),
        ];
        let prefix = select("tag type", tags);
        let label = input("what is the tag label", Feat::NonEmpty);
        target.add_tag(Tag::from(&prefix, &label));
    }
    // description
    if Yes == confirm("do you want to edit the description?", No) {
        match editor(&target.description) {
            Some(txt) => target.description = txt,
            None => {}
        }
    }
    // name
    if Yes == confirm("do you want to edit the name?", No) {
        let prompt = format!("what's the new name for {}?", target.name());
        target.name = input(&prompt, NonEmpty)
    }
    // save
    if Yes == confirm("shall I save the changes?", Yes) {
        ds.update(&target).ok();
    }
}

pub fn edit_entity(ds: &mut DataStore, target: &Entity) -> Entity {
    let mut target = target.clone();
    match select_opt(
        "what do you want to change",
        vec![("Next action", "action"), ("Data", "data")],
    ) {
        Some("action") => {
            edit_next_action(&mut target);
            println!(
                "I'll remind you on {} about {} with:\n{}",
                target.next_action_date,
                target.name(),
                target.next_action_note
            );
        }
        Some("data") => edit_data(ds, &mut target),
        _ => {}
    }

    target
}

pub fn select_actor_role(entity: &Entity) -> Actor {
    // match the options
    let prefix = select(
        "in which capacity?",
        vec![
            ("Lead/Main", "lead"),
            ("Participant", "star"),
            ("Context/Background", "back"),
        ],
    );
    Actor::from(&prefix, &entity.uid()).unwrap()
}

/// Search an entity in the datastore
pub fn search(ds: &DataStore, q: &str) -> Option<Entity> {
    loop {
        let pattern = input(q, Empty);
        match pattern.as_str() {
            "" => {
                return None;
            }
            p => {
                let res = ds.search(p);
                if res.is_empty() {
                    continue;
                }
                match select_entity("please select one  (or esc/q to cancel):", &res) {
                    Some(r) => return Some(r.clone()),
                    None => continue,
                }
            }
        }
    }
}

pub fn select_relationship(target: &Entity) -> Rel {
    // TODO: implement this interaction
    Rel::new(target)
}

pub fn select_entity<'a>(q: &'a str, entities: &'a [Entity]) -> Option<&'a Entity> {
    let opts = entities.iter().map(|e| (e.name(), e)).collect();
    select_opt(q, opts)
}

pub fn select_context(context_manager: &ContextManager) -> String {
    select(
        "Which one?",
        context_manager
            .list()
            .iter()
            .map(|(k, _)| (&k[..], k))
            .collect(),
    )
    .to_owned()
}

pub fn menu() -> Option<String> {
    // ask for the quality
    match select_opt(
        "hello there, what shall we do? esc/q to quit",
        vec![
            ("Quick note", "note"),
            ("Actionable", "today"),
            ("Audit", "inspect"),
            ("Update", "update"),
            ("Overview", "agenda"),
            ("Add new", "add"),
            ("Suggest what to do", "hint"),
            ("Change context", "change_context"),
            ("New context", "new_context"),
        ],
    ) {
        Some(x) => Some(x.to_string()),
        _ => None,
    }
}
