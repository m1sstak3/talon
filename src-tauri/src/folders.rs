//! Folders («Папки», owner 11.10): named groups of accounts — own ones and watch-only addresses — kept in a plain file
//! next to the vault. Not secret: they only group what the wallet already shows. A folder is what a hand-made batch
//! («Перевести со всех») and an agent read from; signing still goes through the vault and the owner's confirmation.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MAX_FOLDERS: usize = 50;
pub const MAX_MEMBERS: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Folder { pub id: String, pub name: String, pub accounts: Vec<String> }

/// the file as the owner left it; none yet = no folders. A broken file is an error, never silently emptied
pub fn read(file: &Path) -> Result<Vec<Folder>, String> {
    match std::fs::read_to_string(file) {
        // checked like a save: another process may have written it (an agent finds folders by name)
        Ok(t) => { let l: Vec<Folder> = serde_json::from_str(&t).map_err(|_| "файл папок повреждён".to_string())?; check(&l)?; Ok(l) }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err("файл папок не читается".into()),
    }
}

/// what the window sends is checked before it lands on disk: sizes, names, unique ids and members
pub fn check(list: &[Folder]) -> Result<(), String> {
    if list.len() > MAX_FOLDERS { return Err(format!("не больше {MAX_FOLDERS} папок")) }
    let mut ids = std::collections::HashSet::new();
    let mut names = std::collections::HashSet::new();
    for f in list {
        let n = f.name.trim().chars().count();
        if n == 0 || n > 40 { return Err("название папки: от 1 до 40 знаков".into()) }
        if !names.insert(f.name.trim().to_lowercase()) { return Err(format!("папка «{}» уже есть", f.name.trim())) }
        if f.id.is_empty() || f.id.len() > 40 || !ids.insert(&f.id) { return Err("у папок должны быть разные номера".into()) }
        if f.accounts.len() > MAX_MEMBERS { return Err(format!("в папке не больше {MAX_MEMBERS} счетов")) }
        let mut seen = std::collections::HashSet::new();
        if f.accounts.iter().any(|a| a.is_empty() || a.len() > 120 || !seen.insert(a)) { return Err(format!("в папке «{}» счёт указан дважды или пустой", f.name.trim())) }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn f(id: &str, name: &str, a: &[&str]) -> Folder { Folder { id: id.into(), name: name.into(), accounts: a.iter().map(|s| s.to_string()).collect() } }
    #[test]
    fn folders_are_checked() {
        assert!(check(&[f("a", "Фермы", &["w/0", "w/1"]), f("b", "Личные", &[])]).is_ok());
        assert!(check(&[f("a", " ", &[])]).is_err(), "empty name");
        assert!(check(&[f("a", "x", &[]), f("a", "y", &[])]).is_err(), "same id twice");
        assert!(check(&[f("a", "x", &["w/0", "w/0"])]).is_err(), "member twice");
        assert!(check(&[f("a", "Фермы", &[]), f("b", "фермы ", &[])]).is_err(), "same name twice (case, spaces)");
        let dir = std::env::temp_dir().join(format!("talon-folders-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("folders.json");
        assert_eq!(read(&file).unwrap(), vec![], "no file = no folders");
        std::fs::write(&file, "{broken").unwrap();
        assert!(read(&file).is_err(), "a broken file is never read as empty");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
