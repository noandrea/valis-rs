use ::valis::{self, Entity, Event, EventType, Tag};
use chrono::NaiveDate;
use rand::random;
use simsearch::SimSearch;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, LineWriter, Write};
use std::path::Path;

use super::utils;

const TABLE_ENTITIES: &str = "ENTITIES";
const TABLE_TAGS: &str = "TAGS";
const TABLE_ACL: &str = "ACL";
const TABLE_EDGES: &str = "EDGES";
const TABLE_ACTIONS: &str = "ACTIONS";
const TABLE_IDS: &str = "IDS";
const TABLE_SYSTEM: &str = "SYSTEM";

const TABLE_EVENTS: &str = "EVENTS";
const TABLE_ENTITY_EVENT: &str = "ENTITY_EVENT";

// Let's use generic errors
type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, Clone, PartialEq)]
pub enum DataError {
    NotImplemented,
    InvalidSponsor,
    NotFound,
    GenericError(String),
    InitializationError,
    IDAlreadyTaken,
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

#[derive(PartialEq)]
pub enum ExportFormat {
    Json,
    NQuad,
}

fn action_key(e: &Entity) -> String {
    format!("{}:{}", e.next_action_date, e.uid())
}
fn tag_key(t: &Tag, e: &Entity) -> String {
    format!("{}:{}:{}", t.prefix(), t.slug(), e.uid())
}
fn handle_key(p: &str, v: &str) -> String {
    utils::hash(&valis::slugify(format!("{}:{}", p, v)))
}
fn str(v: &sled::IVec) -> String {
    String::from_utf8_lossy(v).to_string()
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
    system: sled::Tree,
    events: sled::Tree,
    entity_event: sled::Tree,
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
        let system = db.open_tree(TABLE_SYSTEM)?;
        // events
        let events = db.open_tree(TABLE_EVENTS)?;
        let entity_event = db.open_tree(TABLE_ENTITY_EVENT)?;
        // generate salt for password
        let salt: String = (0..64).map(|_| random::<char>()).collect();
        let salt_hash: &str = &utils::hash(&salt);
        system.insert("password:salt", salt_hash)?;
        // datastore
        Ok(DataStore {
            db,
            entities,
            actions,
            ids,
            tags,
            edges,
            acl,
            system,
            events,
            entity_event,
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

        if format == ExportFormat::NQuad {
            return Err(DataError::NotImplemented);
        }

        match format {
            ExportFormat::Json => self.entities.iter().for_each(|r| {
                let (_, raw) = r.unwrap();
                let e: Entity = bincode::deserialize(&raw).unwrap();
                let j = serde_json::to_string(&e).unwrap();
                file.write(j.as_bytes()).ok();
                file.write("\n".as_bytes()).ok();
            }),
            _ => {}
        };
        file.flush()?;
        Ok(())
    }

    pub fn import(&mut self, path: &Path, format: ExportFormat) -> Result<()> {
        if format == ExportFormat::NQuad {
            return Err(DataError::NotImplemented);
        }
        let file = File::open(path)?;

        match format {
            ExportFormat::Json => BufReader::new(file).lines().for_each(|r| {
                let line = r.unwrap();
                let e: Entity = serde_json::from_str(&line).unwrap();
                self.insert(&e).unwrap();
            }),
            _ => {}
        };
        Ok(())
    }

    /// Perform a search for a string in tags and transaction name
    ///
    pub fn search(&self, pattern: &str) -> Vec<Entity> {
        vec![]
    }

    /// Get a list of events for an entity
    ///
    /// Retrieve the list of events for a n entity
    pub fn events(&self, subject: &Entity, include_logs: bool) -> Vec<Event> {
        let prefix: &str = &subject.uid();
        self.entity_event
            .scan_prefix(prefix)
            .map(|r| {
                let (_k, v) = r.unwrap();
                println!("{} / {} -> {}", prefix, str(&_k), str(&v));
                let raw = self.events.get(v).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .filter(|e: &Event| include_logs || !e.kind.is_log())
            .collect()
    }

    /// Records an event
    ///
    /// An event is recorded in the tree events that is
    /// <uid, Event>
    /// and for all the actors in the entity_event as
    /// <actor_uid:event_uid, event_uid>
    fn record(&mut self, event: &Event) -> Result<valis::Uuid> {
        // consistency check
        if event.actors.is_empty() {
            return Err(DataError::GenericError("no actors for event".to_string()));
        }
        // serialize
        let k: &str = &event.uid();
        let v = bincode::serialize(event).unwrap();
        // insert the event
        self.events.insert(k, v).expect("cannot record event");
        // record the connection between event and entity
        for actor in event.actors.iter() {
            // now insert <actor_uid:event_uid, event_uid>
            let ak: &str = &format!(
                "{}:{}:{}",
                actor.uid(),
                event.recorded_at.timestamp(),
                event.uid()
            );
            self.entity_event.insert(ak, k)?;
        }
        Ok(event.uid)
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

    pub fn agenda_until(&self, until: &NaiveDate, limit: usize, offset: usize) -> Vec<Entity> {
        self.actions
            .iter()
            .map(|r| {
                let (_k, v) = r.unwrap();
                let raw = self.entities.get(v).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .filter(|e: &Entity| e.action_within(until))
            .collect::<Vec<Entity>>()
    }

    /// Return aggregation summary for tags
    ///
    pub fn agenda(
        &self,
        since: &NaiveDate,
        until: &NaiveDate,
        limit: usize,
        offset: usize,
    ) -> Vec<Entity> {
        let prefix_str = utils::prefix(&since.to_string(), &until.pred().to_string());
        // fetch all the stuff
        self.actions
            .scan_prefix(prefix_str)
            .map(|r| {
                let (_k, v) = r.unwrap();
                let raw = self.entities.get(v).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .filter(|e: &Entity| {
                // TODO: also match disabled records
                e.action_within_range(since, until)
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
        let uid = self.insert(principal)?;
        // create a event log
        self.record(&Event::log("init", principal, None))?;
        // return the entity uid
        Ok(uid)
    }

    /// Adds a new entity to the database
    pub fn add(&mut self, entity: &Entity) -> Result<valis::Uuid> {
        // search for the sponsor
        match self.get_by_uid(&entity.sponsor_uid())? {
            Some(sponsor) => {
                // cannot self sponsor
                if sponsor.uid() == entity.uid() {
                    return Err(DataError::InvalidSponsor);
                }
                Ok(())
            }
            None => Err(DataError::InvalidSponsor),
        }?;
        // now check for conflicting ids
        for (label, id) in entity.handles.iter() {
            if self.ids.get(&handle_key(label, id))?.is_some() {
                return Err(DataError::IDAlreadyTaken);
            }
        }
        // all good
        let uid = self.insert(entity)?;
        // create a event log
        self.record(&Event::log("added", entity, None))?;
        // return the entity uid
        Ok(uid)
    }

    pub fn update(&mut self, entity: &Entity) -> Result<valis::Uuid> {
        // search for the sponsor
        match self.get_by_uid(&entity.uid())? {
            Some(old) => {
                // remove existing action dates if they have changed
                if old.next_action_date != entity.next_action_date {
                    self.actions.remove(&action_key(&old))?;
                }
                // remove existing tags if exists
                for (k, t) in old.tags.iter() {
                    if !entity.tags.contains_key(k) {
                        self.tags.remove(&tag_key(t, entity))?;
                    }
                }
                // remove existing ids
                for (k, v) in old.handles.iter() {
                    if !entity.handles.contains_key(k) {
                        self.ids.remove(&handle_key(k, v))?;
                    }
                }
                // now check for conflicting ids
                for (k, v) in entity.handles.iter() {
                    println!("{:?}", k);
                    if let Some(uid) = self.ids.get(&handle_key(k, v))? {
                        if str(&uid) != entity.uid() {
                            return Err(DataError::IDAlreadyTaken);
                        }
                    }
                }
                self.insert(entity)
            }
            None => Err(DataError::NotFound),
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
        let ak = action_key(entity);
        self.actions.insert(ak, k).expect("cannot insert action");
        // insert ids
        // first insert the id itself
        self.ids.insert(k, k).expect("cannot insert id");
        // then insert the rest
        entity.handles.iter().for_each(|(m, id)| {
            let ik = handle_key(m, id);
            self.ids.insert(ik, k).expect("cannot insert id");
        });
        // insert tags
        entity.tags.iter().for_each(|(_ts, t)| {
            self.tags
                .insert(tag_key(t, entity), k)
                .expect("cannot insert tag");
        });
        // insert relations
        entity.relationships.iter().for_each(|r| {
            let ik = format!("{}:{}", entity.uid(), r.kind.get_label());
            let v: &str = &utils::id(&r.target);
            self.edges.insert(ik, v).expect("cannot insert edge");
        });
        // insert acl
        entity.visibility.iter().for_each(|a| {
            let ik = format!("{}:{}", a, k);
            self.acl.insert(ik, k).expect("cannot insert acl");
        });
        Ok(entity.uid)
    }
}
#[cfg(test)]
mod tests {
    use super::utils::*;
    use super::*;
    use valis::*;

    #[test]
    fn test_import_export() {
        let d = tempdir::TempDir::new("valis").unwrap();
        let p = d.path().join("export.json");
        // create a datastore
        let mut orig = DataStore::open(&d.path().join("orig")).unwrap();
        // insert records
        let e = Entity::from("bob", "person")
            .unwrap()
            .self_sponsored()
            .with_handle("email", "bob@acme.com");
        assert_eq!(orig.insert(&e).is_ok(), true);
        let e = Entity::from("alice", "person")
            .unwrap()
            .with_sponsor(&e)
            .with_handle("email", "alice@acme.com");
        assert_eq!(orig.insert(&e).is_ok(), true);
        // now export
        assert_eq!(orig.export(&p, ExportFormat::Json).is_ok(), true);
        // create a new datastore
        let mut copy = DataStore::open(&d.path().join("copy")).unwrap();
        // import
        assert_eq!(copy.import(&p, ExportFormat::Json).is_ok(), true);
        // test
        assert_eq!(orig.entities.len(), copy.entities.len());
        for r in orig.entities.iter() {
            let (k, v) = r.unwrap();
            assert_eq!(copy.entities.get(k).unwrap().unwrap(), v);
        }
    }

    #[test]
    fn test_datastore() {
        let d = tempdir::TempDir::new("valis").unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // insert a records
        let bob = Entity::from("bob", "person").unwrap();
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

    // // TODO: remove
    // assert_eq!(ds.events.len(), 2);
    // println!("owner:{}", owner.uid());
    // for r in ds.entity_event.iter() {
    //     let (k, v) = r.unwrap();
    //     println!("{}  -  {}", str(&k), str(&v));
    // }

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
        // now count events

        let evts = ds.events(&owner, true);
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].actors[0].uid(), owner.uid());

        // insert data
        let data = vec![
            ("A", "person", "01.01.2021", &owner),
            ("B", "person", "02.01.2021", &owner),
            ("C", "person", "01.02.2021", &owner),
            ("D", "person", "02.02.2021", &owner),
        ];
        data.iter().for_each(|(name, class, nad, sp)| {
            let mut e = Entity::from(name, class).unwrap().with_sponsor(sp);
            e.next_action(utils::date_from_str(nad).unwrap(), "yea".to_string());
            ds.insert(&e).unwrap();
        });

        // test
        let (s, u) = TimeWindow::Day(1).range(&utils::date(1, 1, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 1);

        let (s, u) = TimeWindow::Day(2).range(&utils::date(1, 1, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 2);

        let (s, u) = TimeWindow::Year(1).range(&utils::date(1, 1, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 6);

        let (s, u) = TimeWindow::Year(1).range(&utils::date(1, 2, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 2);

        ds.close();
    }

    #[test]
    fn test_update() {
        let d = tempdir::TempDir::new("valis").unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // insert bob
        let bob = Entity::from("bob", "person")
            .unwrap()
            .self_sponsored()
            .with_next_action(date(1, 1, 2000), "something".to_string());

        assert_eq!(ds.insert(&bob).is_ok(), true);
        // now update bob next action
        let bob = bob.with_next_action(date(11, 1, 2000), "something".to_string());
        assert_eq!(ds.update(&bob).is_ok(), true);
        // check that there is only one action in the db
        assert_eq!(ds.actions.len(), 1);
        // now add alice
        let alice = Entity::from("alice", "person")
            .unwrap()
            .self_sponsored()
            .with_handle("email", "alice@acme.com");
        assert_eq!(ds.insert(&alice).is_ok(), true);
        // and bob tries to hijack alice
        let bob = bob.with_handle("email", "alice&acme.com");
        //assert_eq!(ds.update(&bob).is_err(), true);
        assert_eq!(ds.update(&bob).err().unwrap(), DataError::IDAlreadyTaken);
        // // but what if a new player arrives and tries to hijack alice?
        let martha = Entity::from("martha", "person")
            .unwrap()
            .with_sponsor(&bob)
            .with_handle("email", "alice@acme.com");
        assert_eq!(ds.add(&martha).err().unwrap(), DataError::IDAlreadyTaken);
    }
}
