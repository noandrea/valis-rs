use ::valis::{self, Entity};
use chrono::NaiveDate;
use simsearch::SimSearch;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, LineWriter, Write};
use std::path::Path;

/// A simple datastore that can persist data on file
///
pub struct DataStore {
    data: HashMap<blake3::Hash, Entity>,
    index: SimSearch<blake3::Hash>,
}
impl DataStore {
    /// Initialize an empty datastore
    ///
    pub fn new() -> DataStore {
        DataStore {
            data: HashMap::new(),
            index: SimSearch::new(),
        }
    }
    /// Load the datastore with the records found
    /// at log_file path
    pub fn load(&mut self, log_file: &Path) -> Result<(), std::io::Error> {
        // read path
        if let Ok(lines) = DataStore::read_lines(log_file) {
            for line in lines {
                let record = line?;
                if let Ok(tx) = Entity::from_str(&record) {
                    let th = Self::hash(&tx);
                    // index for search the title and the tags
                    self.index.insert(
                        th,
                        &format!("{} {}", tx.get_name(), tx.get_tags().join(" ")),
                    );
                    // here is the move
                    self.data.insert(th, tx);
                }
            }
        }
        Ok(())
    }
    /// Persist the datastore to disk, overwriting existing files
    ///
    /// The order of the item saved is random
    pub fn save(&self, log_file: &Path) -> Result<(), std::io::Error> {
        let mut file = LineWriter::new(File::create(log_file)?);
        self.data.iter().for_each(|v| {
            file.write(v.1.to_string().as_bytes()).ok();
        });
        file.flush()?;
        Ok(())
    }
    /// Retrieve the cost of life for a date
    ///
    pub fn cost_of_life(&self, d: &NaiveDate) -> f32 {
        0.0
    }
    /// Perform a search for a string in tags and transaction name
    ///
    pub fn search(&self, pattern: &str) -> Vec<(String, f32, f32, String, String, f32, String)> {
        self.index
            .search(pattern)
            .iter()
            .map(|h| {
                let tx = self.data.get(h).unwrap();
                (
                    tx.get_name().to_string(),
                    0.0,
                    0.0,
                    "".to_string(),
                    "".to_string(),
                    0.0,
                    tx.get_tags().join("/"),
                )
            })
            .collect()
    }
    /// Compile a summary of the active costs, returning a tuple with
    /// (title, total amount, cost per day, percentage payed)
    pub fn summary(&self, d: &NaiveDate) -> Vec<(String, f32, f32, f32)> {
        let mut s = self
            .data
            .iter()
            .filter(|(_k, v)| true)
            .map(|(_k, v)| {
                (
                    String::from(v.get_name()),
                    0.0,
                    0.0,
                    v.get_progress(&Some(*d)),
                )
            })
            .collect::<Vec<(String, f32, f32, f32)>>();
        // sort the results descending by completion
        s.sort_by(|a, b| (b.3).partial_cmp(&a.3).unwrap());
        s
    }
    /// Return aggregation summary for tags
    ///
    pub fn tags(&self, d: &NaiveDate) -> Vec<(String, usize, f32)> {
        // counters here
        let mut agg: HashMap<String, (usize, usize)> = HashMap::new();
        // aggregate tags
        self.data
            .iter()
            // .filter(|(_h, tx)| tx.is_active_on(d))
            .for_each(|(_h, tx)| {
                tx.get_tags().iter().for_each(|tg| {
                    let (n, a) = match agg.get(tg) {
                        Some((n, a)) => (n + 1, 1),
                        None => (1, 1),
                    };
                    agg.insert(tg.to_string(), (n, a));
                    // * agg.entry(*tg).or_insert((1, tx.per_diem())) +=(1, tx.per_diem());
                });
            });
        // return
        let mut s = agg
            .iter()
            .map(|(tag, v)| (tag.to_string(), v.0, 0.0))
            .collect::<Vec<(String, usize, f32)>>();
        // sort the results descending by count
        s.sort_by(|a, b| (b.2).partial_cmp(&a.2).unwrap());
        return s;
    }
    /// Insert a new tx record
    /// if the record exists returns the existing one
    ///
    /// TODO: handle duplicates more gracefully
    pub fn insert(&mut self, tx: &Entity) -> Option<Entity> {
        let th = Self::hash(tx);
        // index for search the title and the tags
        self.index.insert(
            th,
            &format!("{} {}", tx.get_name(), tx.get_tags().join(" ")),
        );
        self.data.insert(th, tx.clone())
    }
    /// Get the size of the datastore
    ///
    /// # Arguments
    ///
    /// * `on` - A Option<chrono:NaiveDate> to filter for active transactions
    ///
    /// if the Option is None then the full size is returned
    ///
    pub fn size(&self, on: Option<NaiveDate>) -> usize {
        match on {
            Some(date) => self.summary(&date).len(),
            None => self.data.len(),
        }
    }
    // The output is wrapped in a Result to allow matching on errors
    // Returns an Iterator to the Reader of the lines of the file.
    fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
    where
        P: AsRef<Path>,
    {
        let file = File::open(filename)?;
        Ok(io::BufReader::new(file).lines())
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
    use ::valis::{self, Entity};
    #[test]
    fn test_datastore() {}
}
