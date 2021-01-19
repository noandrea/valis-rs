mod ledger;
use ledger::{DataStore, ExportFormat};
mod prompts;
use prompts::{PolarAnswer::*, UserConfig};
mod utils;

use clap::{App, Arg};
use directories::ProjectDirs;
use pad::{Alignment, PadStr};

use std::error;
use std::fs;
use std::path::Path;

use ::valis::{Tag, TimeWindow};
use chrono::NaiveDate;
use Alignment::*;
use Cell::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "farcast";
const APPLICATION: &str = "valis";
const DB_FOLDER: &str = "data";
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
        .subcommand(
            App::new("add")
                .about("add new thing")
                .arg(
                    Arg::new("EXP_STR")
                        .about("write the expense string")
                        .multiple(true)
                        .value_terminator("."),
                )
                .arg(
                    Arg::new("non_interactive")
                        .long("yes")
                        .short('y')
                        .takes_value(false)
                        .about("automatically reply yes"),
                ),
        )
        .subcommand(App::new("today").about("print the today agenda"))
        .subcommand(App::new("export").about("export the database"))
        .subcommand(App::new("import").about("import the database"))
        .subcommand(App::new("agenda").about("print th expenses summary"))
        .subcommand(
            App::new("search").about("search for a transaction").arg(
                Arg::new("SEARCH_PATTERN")
                    .about("pattern to match for tags and/or tx name")
                    .required(true)
                    .multiple(true)
                    .value_terminator("."),
            ),
        )
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
    // Load the datastore
    let db_path = dirs.data_dir().join(Path::new(DB_FOLDER));
    let mut ds = DataStore::open(db_path.as_path())?;
    // import command first of all
    if let Some(("import", c)) = matches.subcommand() {
        let default_path = dirs
            .data_dir()
            .join("export.json")
            .to_string_lossy()
            .to_string();
        let export_path = c.value_of("path").unwrap_or(&default_path);
        ds.import(Path::new(export_path), ExportFormat::Json)?;
        println!("dataset imported from {}", export_path);
        return Ok(());
    }

    // this is instead the config path
    let cfg_path = dirs.preference_dir().join(CFG_USER);
    // fist check if the datastore has content
    // if the datastore is empty then setup
    if ds.is_empty() {
        // if no exit
        if let No = prompts::confirm("VALIS is not configured yet, shall we do it?", Yes) {
            println!("alright, we'll think about it later");
            return Ok(());
        }
        println!("let's start with a few questions");
        // first create the owner itself
        let principal = prompts::principal_entity()
            .self_sponsored()
            .tag(Tag::System("owner".to_owned()))
            .tag(Tag::System("admin".to_owned()));
        // ask about the root entity
        let root = prompts::root_entity()
            .with_sponsor(&principal)
            .tag(Tag::System("root".to_owned()));

        ds.init(&principal)?;
        ds.add(&root)?;
        // now create a new user config and store it
        let cfg = UserConfig::new(principal.uid());
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

    println!("Welcome back {}", principal);

    // command line
    match matches.subcommand() {
        Some(("add", c)) => {
            let entity = match c.values_of("EXP_STR") {
                Some(values) => {
                    let v = values.collect::<Vec<&str>>().join(" ");
                    valis::Entity::from_str(&v).expect("Cannot parse the input string")
                }
                None => prompts::new_entity(),
            }
            .with_sponsor(&principal);
            // check the values for
            if c.is_present("non_interactive") {
                ds.add(&entity)?;
                return Ok(());
            }
            // print the transaction
            println!("Name     : {}", entity.name());
            // save to the store
            match prompts::confirm("Do you want to add it?", Yes) {
                Yes => match ds.add(&entity) {
                    Ok(uid) => println!("added with uid {}", uid),
                    Err(e) => println!("something went wrong {}", e),
                },
                No => println!("ok, another time"),
            }
        }
        Some(("agenda", _c)) => {
            let mut p = Printer::new(vec![30, 3, 3, 13, 80]);

            let ranges = vec![
                ("Past", TimeWindow::UpTo),
                ("Today", TimeWindow::Day(1)),
                ("Tomorrow", TimeWindow::Day(1)),
                ("Within a week", TimeWindow::Day(6)),
                ("Within 2 weeks", TimeWindow::Day(7)),
                ("Within 4 weeks", TimeWindow::Day(14)),
            ];

            p.head(vec!["Name", "", "", "Next Date", "Message"]);
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
                        Date(e.next_action_date),
                        Str(e.get_next_action_headline()),
                    ])
                });
                target_date = until;
                p.sep();
            }

            // separator
            p.render();
        }
        Some(("search", c)) => {
            let mut p = Printer::new(vec![40, 12, 8, 11, 11, 30, 40]);

            if let Some(values) = c.values_of("SEARCH_PATTERN") {
                let pattern = values.collect::<Vec<&str>>().join(" ");
                // no results
                let res = ds.search(&pattern);
                if res.is_empty() {
                    println!("No matches found ¯\\_(ツ)_/¯");
                    return Ok(());
                }
                // with results
                p.head(vec!["Item", "Price", "Diem", "Start", "End", "Tags", "%"]);
                p.sep();
                // data
                // separator
                p.sep();
                p.render();
            }
        }
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
        Some(("today", c)) => {
            println!("press Esc or q to quit");
            let mut items = ds.agenda_until(&utils::today(), 0, 0);
            while !items.is_empty() {
                let target = match prompts::edit_entities(&items) {
                    Some(t) => t,
                    None => break,
                };
                // TODO
                let x = prompts::edit_entity(target.clone());
                ds.update(&x)?;
                items = ds.agenda_until(&utils::today(), 0, 0);
            }
        }
        Some((&_, _)) | None => {
            let today = utils::today();
            let items = ds.agenda(&today, &today.succ(), 0, 0);
            if items.is_empty() {
                println!("Nothing for today");
                return Ok(());
            }

            let mut p = Printer::new(vec![30, 3, 3, 3, 13, 80]);
            // title
            p.head(vec![
                "Name",
                "Status",
                "Relationship",
                "Events",
                "Next Date",
                "Message",
            ]);
            p.sep();
            items.iter().for_each(|e| {
                p.row(vec![
                    Str(e.name.to_string()),
                    Str(e.state.emoji()),
                    Str(e.quality.emoji()),
                    Cnt(311),
                    Date(e.next_action_date),
                    Str(e.get_next_action_headline()),
                ])
            });
            // data
            p.sep();
            p.render();
        }
    }

    ds.close();
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
                            Date(v) => v
                                .format("%a, %d.%m.%y")
                                .to_string()
                                .pad(s, ' ', Left, false),
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
