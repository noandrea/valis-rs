use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserConfig {
    pub uid: String,
    pub pwd: Option<String>,
}

impl UserConfig {
    pub fn new(uid: String) -> UserConfig {
        UserConfig { uid, pwd: None }
    }

    pub fn load(path: &Path) -> Result<Option<UserConfig>, std::io::Error> {
        match path.exists() {
            true => {
                let content = fs::read_to_string(path)?;
                let uc: UserConfig = toml::from_str(&content)?;
                Ok(Some(uc))
            }
            false => Ok(None),
        }
    }

    pub fn save(&self, path: &Path) -> Result<&UserConfig, std::io::Error> {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, toml::to_string(self).unwrap())?;
        Ok(self)
    }
}
