use ::valis::{self, Entity, Tag, TimeWindow};
use chrono::{Duration, NaiveDate};
use simsearch::SimSearch;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, LineWriter, Write};
use std::path::Path;
use std::str::FromStr;

const TABLE_ENTITIES: &str = "ENTITIES";
const TABLE_TAGS: &str = "TAGS";
const TABLE_ACL: &str = "ACL";
const TABLE_EDGES: &str = "EDGES";
const TABLE_EVENTS: &str = "EVENTS";
const TABLE_ACTIONS: &str = "ACTIONS";
const TABLE_IDS: &str = "IDS";

// Let's use generic errors
type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, Clone)]
pub enum DataError {
    NotImplemented,
    InvalidSponsor,
    GenericError(String),
    InitializationError,
}

impl Error for DataError {}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<sled::Error> for DataError {
    fn from(error: sled::Error) -> Self {
        DataError::GenericError(error.to_string())
    }
}

impl From<std::io::Error> for DataError {
    fn from(error: std::io::Error) -> Self {
        DataError::GenericError(error.to_string())
    }
}

pub enum ExportFormat {
    Json,
    Binary,
}

/// A simple datastore that can persist data on file
///
pub struct DataStore {
    db: sled::Db,
    entities: sled::Tree,
    actions: sled::Tree,
    ids: sled::Tree,
    tags: sled::Tree,
    edges: sled::Tree,
    acl: sled::Tree,
}

impl DataStore {
    /// Initialize an empty datastore
    ///
    pub fn open(db_path: &Path) -> Result<DataStore> {
        let db = sled::open(db_path)?;
        let entities = db.open_tree(TABLE_ENTITIES)?;
        let actions = db.open_tree(TABLE_ACTIONS)?;
        let ids = db.open_tree(TABLE_IDS)?;
        let tags = db.open_tree(TABLE_TAGS)?;
        let edges = db.open_tree(TABLE_EDGES)?;
        let acl = db.open_tree(TABLE_ACL)?;
        Ok(DataStore {
            db,
            entities,
            actions,
            ids,
            tags,
            edges,
            acl,
        })
    }

    /// return if the database is empty
    pub fn is_empty(&self) -> bool {
        let entities = self.db.open_tree(TABLE_ENTITIES).unwrap();
        entities.len() == 0
    }

    pub fn close(&self) {
        self.db.flush().unwrap();
    }

    pub fn export(&self, path: &Path, format: ExportFormat) -> Result<()> {
        let mut file = LineWriter::new(File::create(path)?);
        match format {
            ExportFormat::Json => self.entities.iter().for_each(|r| {
                let (_, raw) = r.unwrap();
                let e: Entity = bincode::deserialize(&raw).unwrap();
                let j = serde_json::to_string(&e).unwrap();
                file.write(j.as_bytes()).ok();
            }),
            ExportFormat::Binary => {}
        };
        file.flush()?;
        Ok(())
    }

    /// Perform a search for a string in tags and transaction name
    ///
    pub fn search(&self, pattern: &str) -> Vec<Entity> {
        vec![]
    }

    /// Retrieve an entity by one of its ids
    pub fn get_by_id(&self, id: &str) -> Result<Option<Entity>> {
        match self.ids.get(id)? {
            Some(uid) => match self.entities.get(uid)? {
                Some(v) => Ok(Some(bincode::deserialize(&v).unwrap())),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    /// Retrieve an entity its uid
    pub fn get_by_uid(&self, uid: &str) -> Result<Option<Entity>> {
        match self.entities.get(uid)? {
            Some(v) => Ok(Some(bincode::deserialize(&v).unwrap())),
            None => Ok(None),
        }
    }

    /// Return aggregation summary for tags
    ///
    pub fn agenda(
        &self,
        since: &NaiveDate,
        window: &TimeWindow,
        limit: usize,
        offset: usize,
    ) -> Vec<Entity> {
        let (s, u) = window.range_inclusive(since);
        let prefix_str = valis::prefix(&s.to_string(), &u.to_string());
        // fetch all the stuff
        let (s, u) = window.range(since);
        self.actions
            .scan_prefix(prefix_str)
            .filter(|r| {
                let (_k, v) = r.as_ref().unwrap();
                let raw = self.entities.get(v).unwrap().unwrap();
                let e: Entity = bincode::deserialize(&raw).unwrap();
                e.next_action_date >= s && e.next_action_date < u
            })
            .map(|r| {
                let (_k, v) = r.unwrap();
                let raw = self.entities.get(v).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .collect::<Vec<Entity>>()
    }

    /// Initialized the database with a principal identity.
    ///
    /// It requires that the database is empty and checks that the
    /// sponsor and the user id are the same
    pub fn init(&mut self, principal: &Entity) -> Result<valis::Uuid> {
        if !self.is_empty() {
            return Err(DataError::InitializationError);
        }
        if principal.uid() != principal.sponsor_uid() {
            return Err(DataError::InitializationError);
        };
        self.insert(principal)
    }

    /// Adds a new entity to the database
    pub fn add(&mut self, entity: &Entity) -> Result<valis::Uuid> {
        // search for the sponsor
        match self.get_by_uid(&entity.sponsor_uid())? {
            Some(_s) => self.insert(entity),
            None => Err(DataError::InvalidSponsor),
        }
    }

    /// Insert a new entity and associated data
    fn insert(&mut self, entity: &Entity) -> Result<valis::Uuid> {
        // insert data
        let k: &str = &entity.uid();
        let v = bincode::serialize(entity).unwrap();
        // insert the data
        self.entities.insert(k, v).expect("cannot insert entity");
        // insert next action date
        let ak = format!("{}:{}", entity.next_action_date, entity.uid.to_string());
        self.actions.insert(ak, k).expect("cannot insert action");
        // insert ids
        // first insert the id itself
        self.ids.insert(k, k).expect("cannot insert id");
        // then insert the rest
        entity.handles.iter().for_each(|(m, id)| {
            let ik = format!("{}:{}", m, id);
            self.ids.insert(ik, k).expect("cannot insert id");
        });
        // insert tags
        entity.tags.iter().for_each(|(_ts, t)| {
            let tk = format!("{}:{}", t.to_string(), entity.uid.to_string());
            self.tags.insert(tk, k).expect("cannot insert tag");
        });
        // insert relations
        entity.relationships.iter().for_each(|r| {
            let tk = format!("{}:{}", entity.uid.to_string(), r.kind.get_label());
            self.edges.insert(tk, k).expect("cannot insert edge");
        });
        // insert acl
        entity.visibility.iter().for_each(|a| {
            let ak = format!("{}:{}", a, entity.uid.to_string());
            self.acl.insert(ak, k).expect("cannot insert acl");
        });
        Ok(entity.uid)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datastore() {
        let d = tempdir::TempDir::new("valis").unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // insert a records
        let mut bob = Entity::from("bob", "person").unwrap();
        ds.insert(&bob).unwrap();
        assert_eq!(ds.entities.len(), 1);
        // fetch it back
        let bob_1 = ds.get_by_id(&bob.uid()).unwrap().unwrap();
        assert_eq!(bob_1.sponsor, bob.sponsor);
        let bob_1 = ds.get_by_uid(&bob.uid()).unwrap().unwrap();
        assert_eq!(bob_1.sponsor, bob.sponsor);
        // add a custom id
        ds.insert(&bob).unwrap();
        // the db size should be the same
        assert_eq!(ds.entities.len(), 1);
    }

    #[test]
    fn test_setup() {
        let d = tempdir::TempDir::new("valis").unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // owner
        let owner = Entity::from("bob", "person").unwrap().self_sponsored();
        // root object
        let root = Entity::from("acme", "org").unwrap().with_sponsor(&owner);
        // insert
        ds.init(&owner).ok();
        ds.add(&root).ok();

        // setup
        // insert data
        let data = vec![
            ("A", "person", "01.01.2021", &owner),
            ("B", "person", "02.01.2021", &owner),
            ("C", "person", "01.02.2021", &owner),
            ("D", "person", "02.02.2021", &owner),
        ];
        data.iter().for_each(|(name, class, nad, sp)| {
            let mut e = Entity::from(name, class).unwrap().with_sponsor(sp);
            e.next_action(valis::date_from_str(nad).unwrap(), "yea".to_string());
            ds.insert(&e).unwrap();
        });

        // test
        let a = ds.agenda(&valis::date(1, 1, 2021), &TimeWindow::Day(1), 0, 0);
        assert_eq!(a.len(), 1);

        let a = ds.agenda(&valis::date(1, 1, 2021), &TimeWindow::Day(2), 0, 0);
        assert_eq!(a.len(), 2);

        let a = ds.agenda(&valis::date(1, 1, 2021), &TimeWindow::Year(1), 0, 0);
        a.iter().for_each(|e| println!("name {} ", e.name));
        assert_eq!(a.len(), 6);

        let a = ds.agenda(&valis::date(1, 2, 2021), &TimeWindow::Year(1), 0, 0);
        assert_eq!(a.len(), 2);

        ds.close();
    }
}
