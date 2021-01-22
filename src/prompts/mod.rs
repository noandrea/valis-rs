use super::ledger::DataStore;
use super::utils;
use ::valis::{Actor, Entity, Rel, RelQuality, Tag, TimeWindow};
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
            .interact()
            .unwrap(),
    )
}

/// shortcut for Input
pub fn input(q: &str, empty: Feat) -> String {
    Input::with_theme(&ColorfulTheme::default())
        .with_prompt(q)
        .allow_empty(empty.to_bool())
        .interact()
        .unwrap()
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
    Entity::from(&name, "person")
        .unwrap()
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
    Entity::from(&name, class).unwrap()
}

pub fn new_entity() -> Entity {
    println!("let's add a new entity");
    // get the name
    let name = input("how shall we call it?", NonEmpty);
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
    let mut e = Entity::from(&name, class).unwrap();
    // action
    let rtw = utils::random_timewindow(1, 12, Some('w'));
    let tw = select(
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
    let nan = match editor("leave a note for the reminder") {
        Some(x) => x,
        None => "".to_owned(),
    };
    e.next_action(nad, nan);

    println!(
        "I'll remind you on {} about {} with:\n{}",
        e.next_action_date, name, e.next_action_note
    );
    // info
    if let Yes = confirm("would you like to add some details?", Yes) {
        if let Some(desc) = editor(&format!("write a note about {}", name)) {
            e.description = desc;
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
            e = e.with_handle(prefix, &label);
        }
    };

    // return
    e
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

pub fn edit_entity(ds: &mut DataStore, mut target: Entity) -> Entity {
    // action
    let prompt = format!(
        "next action date is {}, do you want to change it?",
        utils::human_date(&target.next_action_date)
    );
    if Yes == confirm(&prompt, No) {
        let rtw = utils::random_timewindow(1, 12, Some('w'));
        let tw = select(
            &"when shall you be reminded about it",
            vec![
                ("Today", "0d"),
                ("Tomorrow", "1d"),
                ("In a week", "1w"),
                ("In two weeks", "2w"),
                ("In one month", "1m"),
                ("In three months", "3m"),
                ("In six months", "6m"),
                ("Later", &rtw),
            ],
        );
        let nad = TimeWindow::from_str(&tw).unwrap().offset(&utils::today());
        let nan = match editor(&target.next_action_note) {
            Some(x) => x,
            _ => "".to_string(),
        };
        target.next_action(nad, nan);
    }
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
        target = match RelQuality::from_emoji(q, utils::today(), None) {
            Some(q) => target.change_quality(q),
            _ => target,
        };
    }
    // -- advanced editing
    if No == confirm("do you want to edit more details?", No) {
        println!("ok");
        return target;
    }
    // relationships
    while Yes == confirm("relationships?", No) {
        if let Some(entity) = search(ds, "select target (enter to cancel)") {
            let rel = select_relationship(&entity);
            target = target.add_relation(&rel);
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
        ];
        let prefix = select("what do you want to set", handles);
        let label = input(&format!("what is the {} handle", prefix), Feat::NonEmpty);
        target = target.with_handle(prefix, &label);
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
        target = target.tag(Tag::from(&prefix, &label));
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

pub fn menu() -> Option<String> {
    // ask for the quality
    match select_opt(
        "hello there, what shall we do? esc/q to quit",
        vec![
            ("Quick note", "note"),
            ("Agenda", "agenda"),
            ("Dig up today", "today"),
            ("Audit", "inspect"),
            ("Update", "update"),
            ("Add new", "add"),
            ("Suggest what to do", "hint"),
        ],
    ) {
        Some(x) => Some(x.to_string()),
        _ => None,
    }
}
