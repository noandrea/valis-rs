use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UserConfig {
    pub uid: String,
    pub pwd: Option<String>,
    pub ctx: String,
}

impl UserConfig {
    pub fn new(uid: String, ctx: String) -> UserConfig {
        UserConfig {
            uid,
            pwd: None,
            ctx,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_user() {
        let d = tempdir::TempDir::new("valis").unwrap();
        let c = d.path().join("config.toml");

        let uc = UserConfig {
            uid: "a".to_owned(),
            pwd: Some("b".to_owned()),
            ctx: "default".to_owned(),
        };
        assert_eq!(uc.save(&c).is_ok(), true);

        let uc2 = UserConfig::load(&c);
        assert_eq!(uc2.is_ok(), true);
        assert_eq!(uc, uc2.unwrap().unwrap());

        //
        let uc = UserConfig::new("xxx".to_owned(), "default".to_owned());
        assert_eq!(uc.ctx, "default".to_owned());
        assert_eq!(uc.pwd, None);
        assert_eq!(uc.uid, "xxx");
    }
}
