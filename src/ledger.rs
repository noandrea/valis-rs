use ::valis::{self, Entity};
use chrono::{Duration, NaiveDate};
use simsearch::SimSearch;
use std::path::Path;

const TABLE_ENTITIES: &str = "ENTITIES";
const TABLE_TAGS: &str = "TAGS";
const TABLE_ACL: &str = "ACL";
const TABLE_EDGES: &str = "EDGES";
const TABLE_EVENTS: &str = "EVENTS";
const TABLE_ACTIONS: &str = "ACTIONS";

/// A simple datastore that can persist data on file
///
pub struct DataStore {
    db: sled::Db,
}
impl DataStore {
    /// Initialize an empty datastore
    ///
    pub fn open(db_path: &Path) -> DataStore {
        let db = sled::open(db_path).unwrap();
        DataStore { db }
    }

    pub fn close(&self) {}

    pub fn export(file: &Path) {}
    pub fn import(file: &Path) {}

    /// Perform a search for a string in tags and transaction name
    ///
    pub fn search(&self, pattern: &str) -> Vec<Entity> {
        vec![]
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
        let (s, u) = (since.to_string(), (*until - Duration::days(1)).to_string());
        let prefix_str = valis::prefix(&s, &u);
        // fetch all the stuff
        let actions = self.db.open_tree(TABLE_ACTIONS).unwrap();
        let entities = self.db.open_tree(TABLE_ENTITIES).unwrap();
        actions
            .scan_prefix(prefix_str)
            .map(|r| {
                let (_k, v) = r.unwrap();
                let raw = entities.get(v).unwrap().unwrap();
                bincode::deserialize(&raw).unwrap()
            })
            .collect::<Vec<Entity>>()
    }
    /// Insert a new entity and associated data
    pub fn insert(&mut self, entity: &Entity) {
        // insert data
        let k = bincode::serialize(&entity.uid).unwrap();
        let v = bincode::serialize(entity).unwrap();
        let index = self.db.open_tree(TABLE_ENTITIES).unwrap();
        index.insert(k.clone(), v).expect("cannot insert entity");
        // insert next action date
        let index = self.db.open_tree(TABLE_ACTIONS).unwrap();
        let ak = format!("{}:{}", entity.next_action_date, entity.uid.to_string());
        index.insert(ak, k.clone());
        // insert tags
        let index = self.db.open_tree(TABLE_TAGS).unwrap();
        entity.tags.iter().for_each(|(_ts, t)| {
            let tk = format!("{}:{}", t.to_string(), entity.uid.to_string());
            index.insert(tk, k.clone()).expect("cannot insert tag");
        });
        // insert relations
        let index = self.db.open_tree(TABLE_EDGES).unwrap();
        entity.relationships.iter().for_each(|r| {
            let tk = format!("{}:{}", entity.uid.to_string(), r.kind.get_label());
            index.insert(tk, k.clone()).expect("cannot insert edge");
        });
        // insert acl
        let index = self.db.open_tree(TABLE_ACL).unwrap();
        entity.visibility.iter().for_each(|a| {
            let ak = format!("{}:{}", a, entity.uid.to_string());
            index.insert(ak, k.clone()).expect("cannot insert acl");
        })
    }

    /// Compute the blake3 has for a Entity
    ///
    /// The hash is calculated on
    /// - name
    /// - lifetime
    /// - starts_on
    /// - amount
    ///
    fn hash(tx: &Entity) -> blake3::Hash {
        let fields = format!("{}", tx.get_name());
        blake3::hash(fields.as_bytes())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_datastore() {
        let mut ds = DataStore::open(Path::new("private/testdb"));

        let data = vec![
            ("A", "person", "01.01.2021"),
            ("B", "person", "02.01.2021"),
            ("C", "person", "01.02.2021"),
            ("D", "person", "02.02.2021"),
        ];

        data.iter().for_each(|(name, class, nad)| {
            let mut e = Entity::from(name, class).unwrap();
            e.next_action(valis::date_from_str(nad).unwrap(), "yea".to_string());
            ds.insert(&e);
        });

        let a = ds.agenda(&valis::date(1, 1, 2021), &valis::date(2, 1, 2021), 0, 0);
        assert_eq!(a.len(), 1);

        let a = ds.agenda(&valis::date(1, 1, 2021), &valis::date(3, 1, 2021), 0, 0);
        assert_eq!(a.len(), 2);

        let a = ds.agenda(&valis::date(1, 1, 2021), &valis::date(3, 1, 2022), 0, 0);
        assert_eq!(a.len(), 4);
    }
}
