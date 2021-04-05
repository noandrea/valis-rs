use super::model::{self, Entity, Event, Tag};
use chrono::NaiveDate;
use rand::random;
use simsearch::SimSearch;
use sled::{transaction::TransactionResult, Batch, Transactional};
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
const TABLE_SPONSORSHIPS: &str = "SPONSORSHIPS";
const TABLE_EVENTS: &str = "EVENTS";
const TABLE_ENTITY_EVENT: &str = "ENTITY_EVENT";

// Let's use generic errors
type Result<T> = std::result::Result<T, DataError>;

#[derive(Debug, Clone, PartialEq)]
pub enum DataError {
    TxError,
    NotImplemented,
    InvalidSponsor,
    NotFound,
    GenericError(String),
    InitializationError,
    IDAlreadyTaken,
    BrokenReference,
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

#[derive(PartialEq)]
pub enum EventFilter {
    Logs,
    Actions,
    LogsWithMessage(String),
    ActionWithSource(String),
    Any,
}

impl EventFilter {
    pub fn matches(&self, evt: &Event) -> bool {
        match self {
            Self::Logs => evt.kind.is_log(),
            Self::LogsWithMessage(m) => evt.kind.is_log() && (evt.kind.val() == *m),
            Self::Actions => !evt.kind.is_log(),
            Self::ActionWithSource(s) => !evt.kind.is_log() && (evt.kind.val() == *s),
            _ => true,
        }
    }
}

fn action_key(e: &Entity) -> String {
    format!("{}:{}", e.next_action_date, e.uid())
}
fn tag_key(t: &Tag, e: &Entity) -> String {
    format!("{}:{}:{}", t.prefix(), t.slug(), e.uid())
}
fn handle_key(p: &str, v: &str) -> String {
    utils::hash(&utils::slugify(format!("{}:{}", p, v)))
}
fn sponsor_key(e: &model::Uuid, sponsor: &model::Uuid) -> String {
    format!("{}:{}", utils::id(sponsor), utils::id(e))
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
    sponsorships: sled::Tree,
    // search index
    index: SimSearch<String>,
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
        let sponsorships = db.open_tree(TABLE_SPONSORSHIPS)?;
        // events
        let events = db.open_tree(TABLE_EVENTS)?;
        let entity_event = db.open_tree(TABLE_ENTITY_EVENT)?;
        // search index
        let index = SimSearch::new();
        // generate salt for passwords
        let salt: String = (0..64).map(|_| random::<char>()).collect();
        let salt_hash: &str = &utils::hash(&salt);
        system.insert("password:salt", salt_hash)?;
        // datastore
        let mut ds = DataStore {
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
            sponsorships,
            index,
        };
        // build the search index
        ds.build_search_index();
        // complete
        Ok(ds)
    }

    fn build_search_index(&mut self) {
        self.index = SimSearch::new();
        self.entities.iter().for_each(|r| {
            let (_, raw) = r.unwrap();
            let e: Entity = bincode::deserialize(&raw).unwrap();

            let data = format!(
                "{} {} {}",
                e.name(),
                e.get_tags().join(" "),
                e.handles
                    .iter()
                    .map(|(_, v)| v.to_string())
                    .collect::<Vec<String>>()
                    .join(" ")
            );
            self.index.insert(e.uid(), &data);
        });
    }

    /// return if the database is empty
    pub fn is_empty(&self) -> bool {
        let entities = self.db.open_tree(TABLE_ENTITIES).unwrap();
        entities.len() == 0
    }

    /// Flush and close the datastore
    ///
    /// be aware that the underling files may
    /// take a moment to actually close the file
    pub fn close(&self) {
        self.db.flush().unwrap();
        drop(&self.db);
    }

    /// Export the dataset in the format expressed by the format parameter
    ///
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

    /// Import the dataset from an export
    pub fn import(&mut self, path: &Path, format: ExportFormat) -> Result<()> {
        if format == ExportFormat::NQuad {
            return Err(DataError::NotImplemented);
        }
        // clean the database before starting
        self.db.clear()?;
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

    /// Set a metadata value
    pub fn set_meta(&mut self, key: &str, val: &str) -> Result<()> {
        let k = format!("meta:{}", key);
        self.system.insert(&k, val)?;
        Ok(())
    }

    /// Get a metadata value
    pub fn get_meta(&mut self, key: &str) -> Option<String> {
        let k = format!("meta:{}", key);
        if let Ok(v) = self.system.get(&k) {
            if let Some(v) = v {
                return Some(str(&v));
            }
        }
        None
    }

    /// Perform a search for a string in tags and transaction name
    ///
    pub fn search(&self, pattern: &str) -> Vec<Entity> {
        self.index
            .search(pattern)
            .iter()
            .map(|id| {
                let raw = self.entities.get(&id).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .collect::<Vec<Entity>>()
    }

    /// Get a list of events for an entity sorted
    /// by date descending (latest first).
    ///
    /// Retrieve the list of events for an entity
    pub fn events(&self, subject: &Entity, filter: EventFilter) -> Vec<Event> {
        self.events_within(subject, filter, None, None)
    }

    /// Get a list of events for an entity sorted by date
    /// descending (latest first) between two dates
    pub fn events_within(
        &self,
        subject: &Entity,
        filter: EventFilter,
        since: Option<NaiveDate>,
        until: Option<NaiveDate>,
    ) -> Vec<Event> {
        let prefix: &str = &subject.uid();
        self.entity_event
            .scan_prefix(prefix)
            .map(|r| {
                let (_k, v) = r.unwrap();
                let raw = self.events.get(v).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .filter(|e: &Event| filter.matches(e) && e.is_between(since, until))
            .collect()
    }

    /// Records an event
    ///
    /// An event is recorded in the tree events that is
    /// <uid, Event>
    /// and for all the actors in the entity_event as
    /// <actor_uid:event_uid, event_uid>
    pub fn record(&mut self, event: &Event) -> Result<model::Uuid> {
        // consistency check
        if event.actors.is_empty() {
            return Err(DataError::GenericError("no actors for event".to_string()));
        }
        // serialize
        let k: &str = &event.uid();
        // prepare batch for entity_event
        let mut ee_batch = Batch::default();
        for actor in event.actors.iter() {
            // consistency check
            if !self.entities.contains_key(actor.uid())? {
                return Err(DataError::BrokenReference);
            }
            // now insert <actor_uid:event_uid, event_uid>
            let ak: &str = &format!(
                "{}:{}:{}",
                actor.uid(),
                i64::MAX - event.recorded_at.timestamp_millis(),
                event.uid()
            );
            ee_batch.insert(ak, k);
        }

        let (e, ee) = (&self.events, &self.entity_event);
        // start a transaction
        let r: TransactionResult<(), DataError> = (e, ee).transaction(|(events, entity_event)| {
            let v = bincode::serialize(event).unwrap();
            // insert the event
            events.insert(k, v)?;
            // record the connection between event and entity
            entity_event.apply_batch(&ee_batch)?;
            Ok(())
        });
        match r {
            Ok(()) => Ok(event.uid),
            Err(_) => Err(DataError::TxError),
        }
    }

    /// Retrieve an entity by one of its ids
    pub fn get_by_id(&self, prefix: &str, id: &str) -> Result<Option<Entity>> {
        match self.ids.get(handle_key(prefix, id))? {
            Some(uid) => match self.entities.get(uid)? {
                Some(v) => Ok(Some(bincode::deserialize(&v).unwrap())),
                None => Err(DataError::BrokenReference),
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

    pub fn agenda_until(&self, until: &NaiveDate, _limit: usize, _offset: usize) -> Vec<Entity> {
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
        _limit: usize,
        _offset: usize,
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
    pub fn init(&mut self, principal: &Entity) -> Result<model::Uuid> {
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
    pub fn add(&mut self, entity: &Entity) -> Result<model::Uuid> {
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

    pub fn update(&mut self, entity: &Entity) -> Result<model::Uuid> {
        // search for the sponsor
        match self.get_by_uid(&entity.uid())? {
            Some(old) => {
                // remove existing action dates if they have changed
                if old.next_action_date != entity.next_action_date {
                    self.actions.remove(&action_key(&old))?;
                }
                // remove existing sponsor
                if old.sponsor != entity.sponsor {
                    let sk = sponsor_key(&entity.uid, &old.sponsor);
                    self.sponsorships.remove(&sk)?;
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
    fn insert(&mut self, entity: &Entity) -> Result<model::Uuid> {
        // insert data
        let k: &str = &entity.uid();
        let v = bincode::serialize(entity).unwrap();
        // insert the data
        self.entities.insert(k, v)?;
        // insert next action date
        let ak = action_key(entity);
        self.actions.insert(ak, k)?;
        // insert ids
        // first insert the id itself
        self.ids.insert(k, k)?;
        // insert sponsorships
        let ik = sponsor_key(&entity.uid, &entity.sponsor);
        self.sponsorships.insert(ik, k)?;
        // insert handles
        for (m, id) in entity.handles.iter() {
            let ik = handle_key(m, id);
            self.ids.insert(ik, k)?;
        }
        // insert tags
        for (_ts, t) in entity.tags.iter() {
            self.tags.insert(tag_key(t, entity), k)?;
        }
        // insert relations
        for r in entity.relationships.iter() {
            let ik = format!("{}:{}", entity.uid(), r.kind.get_label());
            let v: &str = &utils::id(&r.target);
            self.edges.insert(ik, v)?;
        }
        // insert acl
        for a in entity.visibility.iter() {
            let ik = format!("{}:{}", a, k);
            self.acl.insert(ik, k)?;
        }
        // TODO this is extremely expensive and should be changed
        self.build_search_index();
        // done
        Ok(entity.uid)
    }

    pub fn sponsored_by(&self, sponsor: &Entity) -> Vec<Entity> {
        self.sponsorships
            .scan_prefix(&sponsor.uid())
            .map(|r| {
                let (_, v) = r.unwrap();
                let raw = self.entities.get(&str(&v)).unwrap().unwrap();
                let entity: Entity = bincode::deserialize(&raw).unwrap();
                entity
            })
            .collect::<Vec<Entity>>()
    }

    /// There are three main rules for propose edits
    ///
    /// ### Rule #1 - an entity has been postponed too much (avoided)
    ///
    /// This happens when there are are more then 5 consecutive "postponed"
    /// log events
    ///
    /// ### Rule #2 - an entity that has not been updated in a while
    ///
    /// This happens if an entity has not had a log "reviewed" in the last
    /// 3m, or otherwise not been updated in the last 3m
    ///
    /// Rule #3 - an entity misses most of fields
    ///
    /// Every fields (except the name) have a weight, if the
    /// weight is below threshold then the rules apply.
    ///
    /// An entity is reported only for a rule at a time
    ///
    pub fn propose_edits(&self, principal: &Entity) -> Vec<(EditType, Entity)> {
        let mut to_edit: Vec<(EditType, Entity)> = Vec::new();

        // this is how much an item can be postponed in a row
        let avoidance_limit = 5;

        'main: for e in self.sponsored_by(principal).iter() {
            // Rule#1
            let mut consequent_postponed_times = 0;
            // get the last events
            for evt in self.events(e, EventFilter::Logs).iter() {
                if !EventFilter::LogsWithMessage("postponed".to_owned()).matches(evt) {
                    break;
                }
                consequent_postponed_times += 1;
                if consequent_postponed_times >= avoidance_limit {
                    to_edit.push((EditType::Avoided, e.to_owned()));
                    continue 'main;
                }
            }
            // Rule#2
            let last_update = match self
                .events(e, EventFilter::LogsWithMessage("review".to_string()))
                .first()
            {
                None => e.updated_on,
                Some(evt) => evt.recorded_at.naive_local().date(),
            };
            if last_update < utils::today_plus(-180) {
                to_edit.push((EditType::MaybeStale, e.to_owned()));
                continue;
            }
            // Rule#3
            let mut score = 15;
            if !e.is_classified() {
                score -= 5;
            }
            if e.description.is_empty() {
                score -= 1;
            }
            if e.handles.is_empty() {
                score -= 3;
            }
            if e.tags.is_empty() {
                score -= 3;
            }
            if e.updated_on == e.created_on {
                score -= 1;
            }
            if e.relationships.is_empty() {
                score -= 2;
            }
            if score < 9 {
                to_edit.push((EditType::MaybeIncomplete, e.to_owned()));
            }
        }
        to_edit
    }
}

#[derive(Debug)]
pub enum EditType {
    MaybeStale,
    MaybeIncomplete,
    Avoided,
}

#[cfg(test)]
mod tests {
    use super::model::*;
    use super::utils::*;
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_import_export() {
        let d = TempDir::new().unwrap();
        let p = d.path().join("export.json");
        // create a datastore
        let mut orig = DataStore::open(&d.path().join("orig")).unwrap();
        // insert records
        let e = Entity::from("bob")
            .unwrap()
            .self_sponsored()
            .with_handle("email", "bob@acme.com");
        assert_eq!(orig.insert(&e).is_ok(), true);
        let e = Entity::from("alice")
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
        let d = TempDir::new().unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // reopen should not be possible
        assert_eq!(DataStore::open(d.path()).is_err(), true);
        // insert a records
        let bob = Entity::from("bob").unwrap();
        ds.insert(&bob).unwrap();
        assert_eq!(ds.entities.len(), 1);
        // fetch it back
        let bob_1 = ds.get_by_uid(&bob.uid()).unwrap().unwrap();
        assert_eq!(bob_1.sponsor, bob.sponsor);
        let bob_1 = ds.get_by_uid(&bob.uid()).unwrap().unwrap();
        assert_eq!(bob_1.sponsor, bob.sponsor);
        // add a custom id
        ds.insert(&bob).unwrap();
        // the db size should be the same
        assert_eq!(ds.entities.len(), 1);
    }

    #[test]
    fn test_search() {
        let d = TempDir::new().unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // insert a records
        let bob = Entity::from("Bob Marley")
            .unwrap()
            .self_sponsored()
            .with_tag(Tag::from("skill", "singing"))
            .with_tag(Tag::from("group", "The Wailers"));
        assert_eq!(ds.insert(&bob).is_ok(), true);
        let alice = Entity::from("Alice")
            .unwrap()
            .self_sponsored()
            .with_tag(Tag::from("skill", "cards"))
            .with_tag(Tag::from("address", "Wonderland"))
            .with_tag(Tag::from("skill", "singing"));
        assert_eq!(ds.insert(&alice).is_ok(), true);
        // build index
        ds.build_search_index();
        // search for partial
        let s = ds.search("car");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].uid(), alice.uid());
        // no hit
        let s = ds.search("truck");
        assert_eq!(s.len(), 0);
        // fetch alice
        let s = ds.search("Alice");
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].uid(), alice.uid());
        // skill
        let s = ds.search("singing");
        assert_eq!(s.len(), 2);
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
        let d = TempDir::new().unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // owner
        let owner = Entity::from("bob")
            .unwrap()
            .self_sponsored()
            .with_next_action(utils::date(3, 3, 2020), "whatever".to_string());
        // root object
        let root = Entity::from("acme")
            .unwrap()
            .with_sponsor(&owner)
            .with_next_action(utils::date(3, 10, 2020), "whatever".to_string());
        // init error
        assert_eq!(
            ds.init(&root).err().unwrap(),
            DataError::InitializationError
        );

        // init ok
        assert_eq!(ds.init(&owner).is_ok(), true);
        assert_eq!(ds.add(&root).is_ok(), true);
        // check sponsorship (itself and the sponsored)
        assert_eq!(ds.sponsored_by(&owner).len(), 2);
        // count events
        let events = ds.events(&owner, EventFilter::Any);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].actors[0].uid(), owner.uid());

        // insert data
        let data = vec![
            ("A", "person", "01.01.2021", &owner),
            ("B", "person", "02.01.2021", &owner),
            ("C", "person", "01.02.2021", &owner),
            ("D", "person", "02.02.2021", &owner),
        ];
        data.iter().for_each(|(name, class, nad, sp)| {
            let mut e = Entity::from(name)
                .unwrap()
                .with_class(class)
                .with_sponsor(sp);
            e.next_action(utils::date_from_str(nad).unwrap(), "yea".to_string());
            ds.insert(&e).unwrap();
        });

        // test agenda
        let (s, u) = TimeWindow::Day(1).range(&utils::date(1, 1, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 1);

        let (s, u) = TimeWindow::Day(2).range(&utils::date(1, 1, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 2);

        let (s, u) = TimeWindow::Year(1).range(&utils::date(1, 1, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 4);

        let (s, u) = TimeWindow::Year(1).range(&utils::date(1, 2, 2021));
        let a = ds.agenda(&s, &u, 0, 0);
        assert_eq!(a.len(), 2);

        // test agenda until
        let a = ds.agenda_until(&utils::date(31, 10, 2020), 0, 0);
        assert_eq!(a.len(), 2);

        let a = ds.agenda_until(&utils::date(2, 2, 2021), 0, 0);
        assert_eq!(a.len(), 6);

        ds.close();

        // // TODO: db not closed
        // // init error db not empty
        // let mut ds = DataStore::open(d.path()).unwrap();
        // // init error
        // assert_eq!(
        //     ds.init(&owner).err().unwrap(),
        //     DataError::InitializationError
        // );
    }

    #[test]
    fn test_update() {
        let d = TempDir::new().unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // bob
        let bob = Entity::from("bob")
            .unwrap()
            .self_sponsored()
            .with_next_action(date(1, 1, 2000), "something".to_string());

        // update not existing
        assert_eq!(ds.update(&bob).err().unwrap(), DataError::NotFound);
        // insert bob
        assert_eq!(ds.insert(&bob).is_ok(), true);
        // now update bob next action
        let bob = bob.with_next_action(date(11, 1, 2000), "something".to_string());
        assert_eq!(ds.update(&bob).is_ok(), true);
        // check that there is only one action in the db
        assert_eq!(ds.actions.len(), 1);
        // now add alice
        let alice = Entity::from("alice")
            .unwrap()
            .self_sponsored()
            .with_handle("email", "alice@acme.com");
        assert_eq!(ds.insert(&alice).is_ok(), true);
        // and bob tries to hijack alice
        let bob = bob.with_handle("email", "alice&acme.com");
        //assert_eq!(ds.update(&bob).is_err(), true);
        assert_eq!(ds.update(&bob).err().unwrap(), DataError::IDAlreadyTaken);
        // // but what if a new player arrives and tries to hijack alice?
        let martha = Entity::from("martha")
            .unwrap()
            .with_sponsor(&bob)
            .with_handle("email", "alice@acme.com");
        assert_eq!(ds.add(&martha).err().unwrap(), DataError::IDAlreadyTaken);
        // change alice sponsor
        let alice = ds
            .get_by_id("email", "alice@acme.com")
            .unwrap()
            .unwrap()
            .with_sponsor(&bob);
        assert_eq!(ds.update(&alice).is_ok(), true);
        // TODO handles
        // TODO tags
    }

    #[test]
    fn test_relationships() {
        let d = TempDir::new().unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();

        for i in 0..100 {
            let name = format!("e_{}", i);

            let e = Entity::from(&name)
                .unwrap()
                .self_sponsored()
                .with_handle("code", &name)
                .with_next_action(date(1, 1, 2000), "something".to_string());
            assert_eq!(ds.insert(&e).is_ok(), true);
        }
        // create a new entity
        let e = Entity::from("center")
            .unwrap()
            .self_sponsored()
            .with_handle("code", "center")
            .with_next_action(date(1, 1, 2000), "something".to_string());
        // add relationships
        let e = e
            .add_relation_with(
                &ds.get_by_id("code", "e_1").unwrap().unwrap(),
                RelType::RelatedTo,
            )
            .add_relation_with(
                &ds.get_by_id("code", "e_10").unwrap().unwrap(),
                RelType::RelatedTo,
            )
            .add_relation_with(
                &ds.get_by_id("code", "e_50").unwrap().unwrap(),
                RelType::RelatedTo,
            );
        // insert
        assert_eq!(ds.insert(&e).is_ok(), true);
        // fetch
        let e = ds.get_by_id("code", "center").unwrap().unwrap();
        assert_eq!(e.relationships.len(), 3);
        // add a new one
        let e = e.add_relation_with(
            &ds.get_by_id("code", "e_71").unwrap().unwrap(),
            RelType::RelatedTo,
        );
        // update
        assert_eq!(ds.update(&e).is_ok(), true);
        // fetch
        let e = ds.get_by_id("code", "center").unwrap().unwrap();
        assert_eq!(e.relationships.len(), 4);
    }

    #[test]
    fn test_events() {
        let d = TempDir::new().unwrap();
        println!("dir is {:?}", d);
        // open the datastore
        let mut ds = DataStore::open(d.path()).unwrap();
        // bob
        let bob = Entity::from("bob")
            .unwrap()
            .self_sponsored()
            .with_next_action(date(1, 1, 2000), "something".to_string());
        // insert bob
        assert_eq!(ds.insert(&bob).is_ok(), true);
        // record an event without actors
        let res = ds.record(&Event::new());
        assert_eq!(res.err().unwrap(), DataError::BrokenReference);
        // insert a bunch of events elements
        let elements = 1000;
        for i in 0..elements {
            ds.record(&Event::action(
                "count",
                &format!("{}", i),
                1,
                None,
                &[Actor::Lead(bob.uid.clone())],
            ))
            .unwrap();
            // sleep 1ms
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let events = ds.events(&bob, EventFilter::Actions);
        assert_eq!(events.len(), elements);
        for (i, e) in events.iter().enumerate() {
            assert_eq!(
                e.kind,
                EventType::Action("count".to_owned(), format!("{}", elements - 1 - i), 1)
            );
        }
    }
}
