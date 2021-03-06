use ::valis::data::{
    context::{ContextManager, CtxError},
    ledger::{DataError, DataStore, EventFilter, ExportFormat},
    model::{Actor, Entity, Event, TimeWindow},
    utils,
};
mod prompts;
use prompts::{PolarAnswer::*, UserConfig};

use clap::{App, Arg};
use directories_next::ProjectDirs;
use pad::{Alignment, PadStr};

use std::error;
use std::fs;
use std::path::Path;

use chrono::NaiveDate;
use Alignment::*;
use Cell::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "farcast";
const APPLICATION: &str = "valis";
const CFG_USER: &str = "user.toml";

fn main() -> Result<(), Box<dyn error::Error>> {
    //println!("Welcome to CostOf.Life!");

    let matches = App::new(APPLICATION)
        .version(VERSION)
        .author("Andrea G. <no.andrea@gmail.com>")
        .about("keep track of the cost of your daily life")
        .after_help("visit https://meetvalis.com for more info")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .about("Sets a custom config file")
                .takes_value(true),
        )
        .subcommand(App::new("export").about("export the database"))
        .subcommand(App::new("import").about("import the database"))
        .subcommand(App::new("summary").about("prints the agenda summary"))
        .get_matches();

    // first, see if there is the config dir
    let dirs = ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION)
        .expect("error! cannot establish project home dir! ");

    // data path
    if !dirs.data_dir().exists() {
        match prompts::confirm("The VALIS data dir does not exists, can I create it?", Yes) {
            Yes => {
                fs::create_dir_all(dirs.data_dir()).unwrap();
                println!("data folder created at {:?}", dirs.data_dir());
            }
            No => {
                println!("alright then :(");
                return Ok(());
            }
        }
    }
    // Open the context
    let mut ctxm = ContextManager::new(dirs.data_dir())?;
    //let mut ds = DataStore::open(db_path.as_path())?;
    // TODO: import
    // import command first of all
    // if let Some(("import", c)) = matches.subcommand() {
    //     let default_path = dirs
    //         .data_dir()
    //         .join("export.json")
    //         .to_string_lossy()
    //         .to_string();
    //     let export_path = c.value_of("path").unwrap_or(&default_path);
    //     ds.import(Path::new(export_path), ExportFormat::Json)?;
    //     println!("dataset imported from {}", export_path);
    //     return Ok(());
    // }

    // this is instead the config path
    let cfg_path = dirs.config_dir().join(CFG_USER);
    // if the context manager is empty then setup
    if ctxm.is_empty() {
        // if no exit
        if let No = prompts::confirm("VALIS is not configured yet, shall we do it?", Yes) {
            println!("alright, we'll think about it later");
            return Ok(());
        }
        println!("let's start with a few questions");
        // first create the owner itself
        let principal = prompts::principal_entity();
        // ask about the root entity
        let root = prompts::root_entity();
        // add the context to the database
        let context_name = ctxm.new_datastore(&principal, &root)?;
        // now create a new user config and store it
        let cfg = UserConfig::new(principal.uid(), context_name);
        cfg.save(&cfg_path)?;
    }

    // User management
    let mut cfg = match UserConfig::load(&cfg_path)? {
        Some(uc) => uc,
        None => panic!(
            "missing configuration, please restore the configuration at {:?} before continuing",
            &cfg_path
        ),
    };
    // open the datastore
    let mut ds = ctxm.open_datastore(&cfg.ctx)?;

    // load the current user
    let principal = match ds.get_by_uid(&cfg.uid)? {
        Some(u) => u,
        None => panic!("your configured user does not match in the database"),
    };
    // current user must have the password but it can be cached
    let cached_pwd = cfg.pwd.as_ref();
    // check login
    match cached_pwd {
        Some(pwd) => principal.authorized(Some(pwd)),
        None => {
            let pwd = prompts::password("please enter your password");
            principal.authorized(Some(&utils::hash(&pwd)))
        }
    }
    .expect("invalid credentials!");
    // ask for caching
    if cached_pwd.is_none() {
        if let Yes = prompts::confirm("would you like to cache your password?", Yes) {
            cfg.pwd = principal.get_pwd_hash();
            cfg.save(&cfg_path)?;
        };
    };

    // command line
    match matches.subcommand() {
        Some(("export", c)) => {
            let default_path = dirs
                .data_dir()
                .join("export.json")
                .to_string_lossy()
                .to_string();
            let export_path = c.value_of("path").unwrap_or(&default_path);
            ds.export(Path::new(export_path), ExportFormat::Json)?;
            println!("dataset exported in {}", export_path);
        }
        Some(("summary", _)) => {
            let todo = ds.agenda_until(&utils::today(), 0, 0).len();
            println!(
                "There are {} points for the agenda today for the {} context",
                todo, cfg.ctx
            );
        }
        Some((&_, _)) | None => {
            println!("Welcome back {}", principal);
            println!("you are using the {} context", cfg.ctx);
            while let Some(action) = prompts::menu() {
                let out = match action.as_ref() {
                    "note" => add_note(&mut ds, &principal, None),
                    "agenda" => show_agenda(&ds),
                    "today" => edit_today(&mut ds, &principal),
                    "add" => add_entity(&mut ds, &principal),
                    "update" => update_entity(&mut ds, &principal),
                    "inspect" => inspect(&ds),
                    "hint" => hint(&ds, &principal),
                    "change_context" => {
                        // ask for the name
                        cfg.ctx = prompts::select_context(&ctxm);
                        cfg.save(&cfg_path)?;
                        // close current datastore
                        ds.close();
                        ds = ctxm.open_datastore(&cfg.ctx)?;
                        println!("switched to {} context", cfg.ctx);
                        Ok(())
                    }
                    "new_context" => {
                        cfg.ctx = new_context(&mut ctxm, &principal)?;
                        cfg.save(&cfg_path)?;
                        // close current and open the new one
                        ds.close();
                        ds = ctxm.open_datastore(&cfg.ctx)?;
                        println!("switched to {} context", cfg.ctx);
                        Ok(())
                    }
                    _ => Ok(()),
                };
                match out {
                    Err(e) => {
                        println!("ouch, something went wrong: {}", e)
                    }
                    _ => {}
                }
            }
        }
    }

    ds.close();
    Ok(())
}

// Create a new context
fn new_context(ctxm: &mut ContextManager, principal: &Entity) -> Result<String, CtxError> {
    // ask about the root entity
    let root = prompts::root_entity();
    // add the context to the database
    ctxm.new_datastore(&principal, &root)
}

fn hint(ds: &DataStore, principal: &Entity) -> Result<(), DataError> {
    for (t, e) in ds.propose_edits(principal).iter() {
        println!("{:?} - {}", t, e);
    }
    Ok(())
}

fn show_agenda(ds: &DataStore) -> Result<(), DataError> {
    let mut p = Printer::new(vec![30, 3, 3, 4, 13, 80]);

    let ranges = vec![
        ("Past", TimeWindow::UpTo),
        ("Today", TimeWindow::Day(1)),
        ("Tomorrow", TimeWindow::Day(1)),
        ("Within a week", TimeWindow::Day(6)),
        ("Within 2 weeks", TimeWindow::Day(7)),
        ("Within 4 weeks", TimeWindow::Day(14)),
    ];

    p.head(vec!["Name", "", "", "#Evt", "Next Date", "Message"]);
    p.sep();

    let mut target_date = utils::today();
    for range in ranges {
        let (label, r) = range;
        let (since, until) = r.range(&target_date);
        let items = ds.agenda(&since, &until, 0, 0);
        if items.is_empty() {
            continue;
        }
        // print header
        p.head(vec![&format!(" 📅 {} / {} entries", label, items.len())]);
        p.sep();
        // print stuff
        items.iter().for_each(|e| {
            p.row(vec![
                Str(e.name.to_string()),
                Str(e.state.emoji()),
                Str(e.quality.emoji()),
                Cnt(ds.events(e, EventFilter::Actions).len()),
                Date(e.next_action_date),
                Str(e.get_next_action_headline()),
            ])
        });
        target_date = until;
        p.sep();
    }

    // separator
    p.render();
    Ok(())
}

fn inspect(ds: &DataStore) -> Result<(), DataError> {
    while let Some(e) = prompts::search(ds, "search (or enter for cancel)") {
        println!("Name {}", e.name());
        println!("{}", e.description);
        println!("---------------------------------------------");
        println!("Next action on {}:", utils::human_date(&e.next_action_date));
        println!("{}", e.next_action_note);
        println!("---------------------------------------------");
        println!("Handles");
        for (k, h) in e.handles.iter() {
            println!("{:30}|{:30}", k, h);
        }
        println!("---------------------------------------------");
        println!("Tags");
        for t in e.get_tags() {
            println!("{:30}", t);
        }
        println!("---------------------------------------------");
        println!("Events");
        for evt in ds.events(&e, EventFilter::Actions).iter() {
            println!("recorded at {} from {}", evt.recorded_at, evt.kind);
            match &evt.content {
                Some(c) => println!("{}", c),
                None => println!("-no content-"),
            };
            println!(">>>>>>>>>>>>");
            println!("Actors");
            for a in evt.actors.iter() {
                let (title, uid) = a.role();
                let ac = ds.get_by_uid(&utils::id(&uid)).unwrap().unwrap();
                println!("{:10} - {}", title, ac.name());
            }
        }
        println!("---------------------------------------------");
    }
    Ok(())
}

fn update_entity(ds: &mut DataStore, _principal: &Entity) -> Result<(), DataError> {
    while let Some(e) = prompts::search(ds, "search what you want to update") {
        let target = prompts::edit_entity(ds, &e);
        ds.update(&target)?;
    }
    Ok(())
}

fn add_entity(ds: &mut DataStore, principal: &Entity) -> Result<(), DataError> {
    let name = match prompts::input_opt("name? (empty to cancel)") {
        Some(n) => n,
        None => return Ok(()),
    };
    let new = match prompts::new_entity_unless_exists(ds, &name, principal) {
        Some(e) => e,
        None => return Ok(()),
    };
    match prompts::confirm("Do you want to add it?", Yes) {
        Yes => match ds.add(&new) {
            Ok(uid) => println!("added with uid {}", uid),
            Err(e) => println!("something went wrong {}", e),
        },
        No => println!("ok, another time"),
    };
    Ok(())
}

fn edit_today(ds: &mut DataStore, principal: &Entity) -> Result<(), DataError> {
    let mut items = ds.agenda_until(&utils::today(), 0, 0);
    while !items.is_empty() {
        let target = match prompts::edit_entities(&items) {
            Some(t) => t,
            None => break,
        };
        // ask if to add an event
        if Yes == prompts::confirm("do you want to record a note?", No) {
            add_note(ds, principal, Some(&target))?;
        }
        let target = prompts::edit_entity(ds, target);
        ds.update(&target)?;
        items = ds.agenda_until(&utils::today(), 0, 0);
    }
    Ok(())
}

fn add_note(
    ds: &mut DataStore,
    author: &Entity,
    subject: Option<&Entity>,
) -> Result<(), DataError> {
    // if the subject is Some then add the
    // next_action_message as preamble
    let q = match subject {
        Some(s) => format!("{}\n-----\n", s.next_action_note),
        None => "type in your note".to_owned(),
    };
    // ask to edit
    let text = match prompts::editor(&q) {
        Some(text) => text,
        _ => {
            println!("alright aborting");
            return Ok(());
        }
    };
    // search for actors and add them to the event
    let actors = valis::data::find_labels(&text)
        .iter()
        .map(|l| match utils::split_once(l, ':') {
            Some((p, v)) => {
                if let Some((e, is_new)) = prompts::select_or_create(ds, v, author) {
                    if is_new {
                        // TODO this unwrap shall be gone
                        ds.add(&e).unwrap();
                    }
                    // create an actor out of the entity
                    return Some(Actor::from(p, &e.uid()).unwrap());
                }
                return None;
            }
            None => None,
        })
        .collect::<Vec<Option<Actor>>>();

    // create the event
    let mut evt = Event::action(
        "cli",
        "note",
        1,
        Some(text),
        &[Actor::RecordedBy(author.uid)],
    );
    // add all the actors found
    actors.iter().for_each(|a| {
        if let Some(actor) = a {
            evt.actors.push(actor.to_owned());
        }
    });
    // if there was a subject add that one as well
    if let Some(s) = subject {
        evt.actors.push(Actor::Subject(s.uid.clone()))
    }
    while Yes == prompts::confirm("add another actor", No) {
        match prompts::input_opt("name") {
            None => break,
            Some(name) => {
                if let Some((e, is_new)) = prompts::select_or_create(ds, &name, author) {
                    if is_new {
                        // TODO this unwrap shall be gone
                        ds.add(&e).unwrap();
                    }
                    let a = prompts::select_actor_role(&e);
                    evt.actors.push(a);
                }
            }
        }
    }
    ds.record(&evt)?;
    Ok(())
}

#[derive(Debug)]
enum Cell {
    Str(String),     // string
    Date(NaiveDate), // date
    Cnt(usize),
    Sep,
}

#[derive(Debug)]
struct Printer {
    sizes: Vec<usize>,
    data: Vec<Vec<Cell>>,
    col_sep: String,
    row_sep: char,
    progress: char,
}

impl Printer {
    pub fn new(col_sizes: Vec<usize>) -> Printer {
        Printer {
            sizes: col_sizes,
            data: Vec::new(),
            row_sep: '-',
            progress: '▮',
            col_sep: "|".to_string(),
        }
    }

    pub fn row(&mut self, row_data: Vec<Cell>) {
        self.data.push(row_data);
    }

    pub fn head(&mut self, head_data: Vec<&str>) {
        self.row(head_data.iter().map(|v| Str(v.to_string())).collect());
    }

    pub fn sep(&mut self) {
        self.row(self.sizes.iter().map(|_| Sep).collect());
    }

    pub fn to_string(&self) -> String {
        self.data
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let s = self.sizes[i];
                        match c {
                            Str(v) => v.pad(s, ' ', Left, true),
                            Cnt(v) => format!("{}", v).pad(s, ' ', Right, false),
                            Date(v) => utils::human_date(v).pad(s, ' ', Left, false),
                            Sep => "".pad(s, self.row_sep, Alignment::Right, false),
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(&self.col_sep)
            })
            .collect::<Vec<String>>()
            .join("\n")
    }

    pub fn render(&self) {
        println!("{}", self.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_printer() {
        let mut p = Printer::new(vec![5, 10, 10, 50]);
        p.head(vec!["a", "b", "c", "d"]);
        p.sep();
        p.row(vec![
            Str("One".to_string()),
            // Amt(80.0),
            Cnt(100),
        ]);
        p.row(vec![
            Str("Two".to_string()),
            // Amt(59.0),
            Cnt(321),
        ]);
        p.row(vec![
            Str("Three".to_string()),
            // Amt(220.0),
            Cnt(11),
        ]);
        p.sep();
        assert_eq!(p.data.len(), 6);
    }
}
