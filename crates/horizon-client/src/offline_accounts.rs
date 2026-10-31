//! Small, local-only player profile store.
//!
//! These are deliberately not Microsoft accounts and never contain tokens or
//! passwords.  An offline profile is only the Java player name Horizon uses
//! when it joins an offline-mode/LAN server.  Keeping this separate from the
//! launcher lets Android expose a Bedrock-like account picker inside the app
//! without changing Horizon's authentication path.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

pub const FILE: &str = "horizon-offline-accounts.json";
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Disk {
    version: u32,
    selected: String,
    accounts: Vec<Account>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accounts {
    selected: String,
    accounts: Vec<Account>,
}

impl Accounts {
    /// Loads the profile list, importing Horizon's existing `horizon-player.txt`
    /// on first use. Invalid/corrupt JSON is ignored rather than blocking the
    /// game from starting.
    pub fn load_or_create(dir: &Path) -> Self {
        if let Ok(text) = fs::read_to_string(dir.join(FILE))
            && let Ok(disk) = serde_json::from_str::<Disk>(&text)
            && let Some(accounts) = Self::from_disk(disk)
        {
            return accounts;
        }

        let name = crate::offline_name::load_or_create(dir);
        let name = if valid_name(&name) {
            name
        } else {
            crate::offline_name::generate(seed())
        };
        let accounts = Self {
            selected: name.clone(),
            accounts: vec![Account { name }],
        };
        let _ = accounts.save(dir);
        accounts
    }

    fn from_disk(mut disk: Disk) -> Option<Self> {
        if disk.version != VERSION {
            return None;
        }
        disk.accounts.retain(|a| valid_name(&a.name));
        let mut unique = Vec::<Account>::new();
        for account in disk.accounts {
            if !unique
                .iter()
                .any(|saved| saved.name.eq_ignore_ascii_case(&account.name))
            {
                unique.push(account);
            }
        }
        if unique.is_empty() {
            return None;
        }
        let selected = unique
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(&disk.selected))
            .map(|a| a.name.clone())
            .unwrap_or_else(|| unique[0].name.clone());
        Some(Self {
            selected,
            accounts: unique,
        })
    }

    pub fn all(&self) -> &[Account] {
        &self.accounts
    }

    pub fn selected_name(&self) -> &str {
        &self.selected
    }

    pub fn selected_index(&self) -> usize {
        self.accounts
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(&self.selected))
            .unwrap_or(0)
    }

    pub fn add(&mut self, name: &str) -> Result<(), String> {
        let name = checked(name)?;
        if self
            .accounts
            .iter()
            .any(|a| a.name.eq_ignore_ascii_case(&name))
        {
            return Err("That offline account already exists.".into());
        }
        self.accounts.push(Account { name: name.clone() });
        self.selected = name;
        Ok(())
    }

    pub fn select(&mut self, index: usize) -> Result<(), String> {
        let Some(account) = self.accounts.get(index) else {
            return Err("That offline account does not exist.".into());
        };
        self.selected = account.name.clone();
        Ok(())
    }

    pub fn rename(&mut self, index: usize, name: &str) -> Result<(), String> {
        let name = checked(name)?;
        if self
            .accounts
            .iter()
            .enumerate()
            .any(|(i, a)| i != index && a.name.eq_ignore_ascii_case(&name))
        {
            return Err("That offline account already exists.".into());
        }
        let Some(account) = self.accounts.get_mut(index) else {
            return Err("That offline account does not exist.".into());
        };
        let was_selected = account.name.eq_ignore_ascii_case(&self.selected);
        account.name = name.clone();
        if was_selected {
            self.selected = name;
        }
        Ok(())
    }

    /// Removes a profile. Keeping one profile avoids the ambiguous no-identity
    /// state that previously made LAN players collide with one another.
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        if self.accounts.len() <= 1 {
            return Err("Keep at least one offline account.".into());
        }
        if index >= self.accounts.len() {
            return Err("That offline account does not exist.".into());
        }
        let removed_selected = self.accounts[index]
            .name
            .eq_ignore_ascii_case(&self.selected);
        self.accounts.remove(index);
        if removed_selected {
            self.selected = self.accounts[index.min(self.accounts.len() - 1)]
                .name
                .clone();
        }
        Ok(())
    }

    /// Saves atomically and also updates Horizon's legacy single-name file.
    /// That keeps the desktop launcher and every existing code path compatible
    /// while the multi-profile UI remains an additive feature.
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        fs::create_dir_all(dir)?;
        let disk = Disk {
            version: VERSION,
            selected: self.selected.clone(),
            accounts: self.accounts.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&disk).map_err(io::Error::other)?;
        let path = dir.join(FILE);
        let tmp = dir.join(format!("{FILE}.tmp"));
        fs::write(&tmp, bytes)?;
        if let Err(first) = fs::rename(&tmp, &path) {
            if path.exists() {
                fs::remove_file(&path)?;
                fs::rename(&tmp, &path)?;
            } else {
                return Err(first);
            }
        }
        crate::offline_name::store(dir, &self.selected)
    }
}

pub fn valid_name(name: &str) -> bool {
    let len = name.chars().count();
    (3..=16).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn checked(name: &str) -> Result<String, String> {
    let name = name.trim();
    if valid_name(name) {
        Ok(name.to_owned())
    } else {
        Err("Use 3-16 letters, numbers, or underscores.".into())
    }
}

fn seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        ^ u64::from(std::process::id()).rotate_left(32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "horizon-offline-accounts-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn imports_legacy_identity_and_round_trips() {
        let d = dir("migrate");
        fs::write(d.join(crate::offline_name::FILE), "name:Alex_2\n").unwrap();
        let accounts = Accounts::load_or_create(&d);
        assert_eq!(accounts.selected_name(), "Alex_2");
        assert_eq!(
            accounts.all(),
            &[Account {
                name: "Alex_2".into()
            }]
        );
        assert!(d.join(FILE).is_file());
        assert_eq!(Accounts::load_or_create(&d), accounts);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn add_select_rename_remove_and_legacy_sync() {
        let d = dir("manage");
        fs::write(d.join(crate::offline_name::FILE), "name:First_1\n").unwrap();
        let mut accounts = Accounts::load_or_create(&d);
        accounts.add("Second_2").unwrap();
        assert_eq!(accounts.selected_name(), "Second_2");
        accounts.select(0).unwrap();
        accounts.rename(0, "Main_3").unwrap();
        accounts.save(&d).unwrap();
        assert_eq!(
            fs::read_to_string(d.join(crate::offline_name::FILE)).unwrap(),
            "name:Main_3\n"
        );
        accounts.remove(1).unwrap();
        assert!(accounts.remove(0).is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn rejects_invalid_and_duplicate_names() {
        let d = dir("validation");
        let mut accounts = Accounts::load_or_create(&d);
        for name in ["ab", "has space", "way-too-long-player", "été"] {
            assert!(accounts.add(name).is_err(), "accepted {name:?}");
        }
        let selected = accounts.selected_name().to_owned();
        assert!(accounts.add(&selected.to_ascii_lowercase()).is_err());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn corrupt_store_falls_back_without_panicking() {
        let d = dir("corrupt");
        fs::write(d.join(FILE), "{ definitely not json").unwrap();
        fs::write(d.join(crate::offline_name::FILE), "name:Fallback_9\n").unwrap();
        let accounts = Accounts::load_or_create(&d);
        assert_eq!(accounts.selected_name(), "Fallback_9");
        let _ = fs::remove_dir_all(d);
    }
}
