mod ledger;
use ledger::DataStore;
mod prompts;
use prompts::ConfirmAnswer::*;
mod utils;

use clap::{App, Arg};
use directories::ProjectDirs;
use pad::{Alignment, PadStr};

use std::error;
use std::fs;
use std::path::Path;

use Alignment::*;
use Cell::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const QUALIFIER: &str = "com";
const ORGANIZATION: &str = "farcast";
const APPLICATION: &str = "valis";
const DB_FOLDER: &str = "data";

fn main() -> Result<(), Box<dyn error::Error>> {
    //println!("Welcome to CostOf.Life!");

    let matches = App::new(APPLICATION)
        .version(VERSION)
        .author("Andrea G. <no.andrea@gmail.com>")
        .about("keep track of the cost of your daily life")
        .after_help("visit https://thecostof.life for more info")
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
                        .required(true)
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
    let path = match ProjectDirs::from(QUALIFIER, ORGANIZATION, APPLICATION) {
        Some(p) => {
            if !p.data_dir().exists() {
                match prompts::confirm("The VALIS data dir does not exists, can I create it?", Yes)
                {
                    Yes => match fs::create_dir_all(p.data_dir()) {
                        Ok(_) => println!("data folder created at {:?}", p.data_dir()),
                        Err(e) => {
                            println!("error creating folder {:?}: {}", p.data_dir(), e);
                            panic!()
                        }
                    },
                    No => {
                        println!("alright then :(");
                        return Ok(());
                    }
                }
            }
            p.data_dir().join(Path::new(DB_FOLDER))
        }
        None => panic!("cannot retrieve the config file dir"),
    };
    // load the datastores
    let mut ds = DataStore::open(path.as_path());
    // command line
    match matches.subcommand() {
        Some(("add", c)) => {
            if let Some(values) = c.values_of("EXP_STR") {
                let v = values.collect::<Vec<&str>>().join(" ");
                let th = valis::Entity::from_str(&v).expect("Cannot parse the input string");
                // check the values for
                if c.is_present("non_interactive") {
                    ds.insert(&th);
                    println!("done!");
                    return Ok(());
                }
                // print the transaction
                println!("Name     : {}", th.get_name());
                // save to the store
                match prompts::confirm("Do you want to add it?", Yes) {
                    Yes => {
                        ds.insert(&th);
                        println!("done!")
                    }
                    No => println!("ok, another time"),
                }
            } else {
                println!("Tell me what to add, eg: Car 2000€ .transport 5y")
            }
        }
        Some(("today", _c)) => {
            let mut p = Printer::new(vec![27, 12, 9]);
            // title
            p.head(vec!["Name", "Price", "Due"]);
            p.sep();
            // data
            p.sep();
            p.render();
        }
        Some(("agenda", _c)) => {
            let mut p = Printer::new(vec![27, 12, 9, 100]);

            p.head(vec!["Title", "Count", "Diem", "%"]);
            p.sep();
            // total per diem
            // separator
            p.sep();
            p.render();
        }
        Some(("search", c)) => {
            let mut p = Printer::new(vec![40, 12, 8, 11, 11, 30, 40]);

            if let Some(values) = c.values_of("SEARCH_PATTERN") {
                let pattern = values.collect::<Vec<&str>>().join(" ");
                // no results
                let res = ds.search(&pattern);
                if res.len() == 0 {
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
        Some((&_, _)) | None => {}
    }

    ds.close();
    Ok(())
}

#[derive(Debug)]
enum Cell {
    Amt(f32),    // amount
    Pcent(f32),  // percent
    Str(String), // string
    Cnt(usize),  // counter
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
                            Amt(v) => format!("{}€", v).pad(s, ' ', Right, false),
                            Cnt(v) => format!("{}", v).pad(s, ' ', Right, false),
                            Pcent(v) => {
                                let p = v * 100.0;
                                let b = (p as usize * s) / 100; // bar length
                                format!("{:.2}", p).pad(b, self.progress, Right, false)
                            }
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
            Amt(80.0),
            Cnt(100),
            Pcent(0.1043), // completion percentage
        ]);
        p.row(vec![
            Str("Two".to_string()),
            Amt(59.0),
            Cnt(321),
            Pcent(0.0420123123), // completion percentage
        ]);
        p.row(vec![
            Str("Three".to_string()),
            Amt(220.0),
            Cnt(11),
            Pcent(0.309312321), // completion percentage
        ]);
        p.sep();

        let printed =
            "a    |b         |c         |d                                                 
-----|----------|----------|--------------------------------------------------
One  |       80€|       100|10.43
Two  |       59€|       321|4.20
Three|      220€|        11|▮▮▮▮▮▮▮▮▮▮30.93
-----|----------|----------|--------------------------------------------------";

        assert_eq!(p.data.len(), 6);
        assert_eq!(p.to_string(), printed);
    }
}
