use super::{
    ledger::{DataError, DataStore},
    model::{Entity, Tag, Uuid},
    utils,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

// Let's use generic errors
type Result<T> = std::result::Result<T, CtxError>;

#[derive(Debug, Clone, PartialEq)]
pub enum CtxError {
    InvalidContext,
    DatasetNotFound,
    DatasetExists,
    DatasetInUse,
    GenericError(String),
}

impl Error for CtxError {}

impl Display for CtxError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<std::io::Error> for CtxError {
    fn from(error: std::io::Error) -> Self {
        CtxError::GenericError(error.to_string())
    }
}

impl From<DataError> for CtxError {
    fn from(error: DataError) -> Self {
        CtxError::GenericError(error.to_string())
    }
}

const INDEX_FILE: &str = "context.index.toml";

/// system keys
const META_DATASET_NAME: &str = "DATASET_NAME";

#[derive(Debug)]
pub struct ContextManager {
    base_path: PathBuf,
    contexts: BTreeMap<String, String>,
}

/// ContextManager allows to maintain
/// more than one instance of a valis dataset
/// and to switch between them
impl ContextManager {
    /// Returns the number of contexts available
    pub fn size(&self) -> usize {
        self.contexts.len()
    }

    /// Returns whenever the context has any entry
    pub fn is_empty(&self) -> bool {
        self.contexts.is_empty()
    }

    /// Returns a list of contexts with
    /// (name, path)
    pub fn list(&self) -> Vec<(String, String)> {
        self.contexts
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    self.base_path.join(v).to_string_lossy().to_string(),
                )
            })
            .collect::<Vec<(String, String)>>()
    }

    /// create a new context manager starting from a directory
    ///
    /// if the directory is not empty it will try to load the
    /// available contexts from there
    pub fn new(base: &Path) -> Result<ContextManager> {
        let mut ctx = ContextManager {
            base_path: base.to_path_buf(),
            contexts: BTreeMap::new(),
        };
        // if it is not a dir then die
        if !ctx.base_path.is_dir() {
            return Err(CtxError::InvalidContext);
        }
        // if does not exists try to create
        fs::create_dir_all(&ctx.base_path)?;
        // try to load the contexts
        let index_path = base.join(INDEX_FILE);
        if !index_path.exists() {
            ctx.build_index()?;
        }
        Ok(ctx)
    }

    /// Open
    pub fn open_datastore(&self, name: &str) -> Result<DataStore> {
        match self.contexts.get(name) {
            Some(uid) => {
                let path = self.base_path.join(uid);
                if let Ok(ds) = DataStore::open(&path) {
                    return Ok(ds);
                }
                Err(CtxError::DatasetInUse)
            }
            None => Err(CtxError::DatasetNotFound),
        }
    }

    /// Setup a new datastore
    pub fn new_datastore(&mut self, owner: &Entity, root: &Entity) -> Result<String> {
        if self.contexts.contains_key(&String::from(root.name())) {
            return Err(CtxError::DatasetExists);
        }
        // add more coordinates to the owner
        let owner = owner
            .clone()
            .self_sponsored()
            .with_tag(Tag::System("owner".to_owned()))
            .with_tag(Tag::System("admin".to_owned()));
        // add more stuff to the root
        let root = root
            .clone()
            .with_sponsor(&owner)
            .with_tag(Tag::System("root".to_owned()));
        // get the dataset name and uid
        let ds_uid = utils::id(&Uuid::new_v4());
        let ds_name = String::from(root.name());
        // dataset path
        let db_path = self.base_path.join(Path::new(&ds_uid));
        // create the datastore
        let mut ds = DataStore::open(db_path.as_path())?;
        ds.init(&owner)?;
        ds.add(&root)?;
        ds.set_meta(META_DATASET_NAME, root.name())?;
        ds.close();
        // insert the datastore to the context
        self.contexts.insert(ds_name, ds_uid);
        // return the dataset name
        Ok(root.name().to_owned())
    }

    /// Index the base directory searching for the
    /// databases and builds the indexes
    pub fn build_index(&mut self) -> Result<usize> {
        let dirs = fs::read_dir(&self.base_path)?;
        // cycle over files
        for entry in dirs {
            if let Ok(entry) = entry {
                if let Ok(file_type) = entry.file_type() {
                    if !file_type.is_dir() {
                        continue;
                    }
                    // it's a dir and we can open it
                    let uid = entry.file_name().to_string_lossy().to_string();
                    let path = self.base_path.join(entry.file_name());
                    if let Ok(mut ds) = DataStore::open(&path) {
                        let name = ds
                            .get_meta(META_DATASET_NAME)
                            .unwrap_or("default".to_owned());
                        self.contexts.insert(name, uid);
                        // close the dataset
                        ds.close();
                    }
                }
            }
        }
        Ok(self.contexts.len())
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_context_manager() {
        let d = tempdir::TempDir::new("valis").unwrap();
        // try to create a context
        let ctx = ContextManager::new(&d.path());
        assert_eq!(ctx.is_ok(), true);
        let mut ctx = ctx.unwrap();
        // should be empty now
        assert_eq!(ctx.size(), 0);
        assert_eq!(ctx.list().len(), 0);
        // create ds entities
        let owner = Entity::from("bob").unwrap();
        let root = Entity::from("acme").unwrap();
        // add context
        let ds = ctx.new_datastore(&owner, &root);
        assert_eq!(ds.is_ok(), true);
        let _ds = ds.unwrap();
        assert_eq!(ctx.size(), 1);
        assert_eq!(ctx.list().len(), 1);
        // reopen same datastore
        let ds = ctx.open_datastore(root.name());
        assert_eq!(ds.is_err(), false);
        // // reopen same datastore
        let _ds = ctx.open_datastore(root.name());
        assert_eq!(_ds.is_err(), true);
        // add existing context
        let _ds = ctx.new_datastore(&owner, &root);
        assert_eq!(_ds.is_err(), true);
        // add
    }
}
