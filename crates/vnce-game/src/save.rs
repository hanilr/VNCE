//! Save files: [`SaveFile`] keeps best scores, settings and key bindings between runs.
//!
//! The file is plain text, one `key = value` per line, so players can read and edit it.
//! Lines starting with `#` are comments. Missing or broken files never stop a game: they
//! simply start empty, and broken lines are skipped.

use std::collections::BTreeMap;
use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::actions::Binding;

/// The file name used by [`SaveFile::open`].
const FILE_NAME: &str = "save.txt";

/// Values saved between runs of a game.
///
/// ```
/// use vnce_game::SaveFile;
///
/// let mut save = SaveFile::new(); // in memory; use `SaveFile::open("my-game")` in games
/// save.set("best", 1200);
/// save.set("music", 60);
/// assert_eq!(save.get::<u32>("best"), Some(1200));
/// assert_eq!(save.get_or("effects", 80), 80);
/// assert!(save.to_text().contains("best = 1200"));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveFile {
    path: Option<PathBuf>,
    values: BTreeMap<String, String>,
}

impl SaveFile {
    /// An empty save that lives only in memory; [`SaveFile::save`] does nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The save file of the game `app` in the user's data folder (see [`data_dir`]),
    /// loaded if it exists. Without a data folder the save lives only in memory.
    #[must_use]
    pub fn open(app: &str) -> Self {
        data_dir(app).map_or_else(Self::new, |dir| Self::at(dir.join(FILE_NAME)))
    }

    /// The save file at `path`, loaded if it exists and can be read.
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let mut save = fs::read_to_string(&path)
            .map(|text| Self::from_text(&text))
            .unwrap_or_default();
        save.path = Some(path);
        save
    }

    /// Reads `key = value` lines. Comments (`#`), empty lines and lines without `=` are
    /// skipped; when a key appears twice the last value wins.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let values = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
            .filter(|(key, _)| !key.is_empty())
            .collect();
        Self { path: None, values }
    }

    /// The file contents: a comment, then every value sorted by key.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut text = String::from("# Saved game data. One `key = value` per line.\n");
        for (key, value) in &self.values {
            text.push_str(key);
            text.push_str(" = ");
            text.push_str(value);
            text.push('\n');
        }
        text
    }

    /// Where [`SaveFile::save`] writes (`None` for a save in memory).
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Writes the file, creating its folder if needed. The data goes to a temporary file
    /// first, so an interrupted save never destroys the previous one. A save in memory
    /// does nothing.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the folder or the file cannot be written.
    pub fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir)?;
        }
        let mut temporary = path.clone().into_os_string();
        temporary.push(".tmp");
        fs::write(&temporary, self.to_text())?;
        fs::rename(&temporary, path)
    }

    /// The text stored for `key`.
    #[must_use]
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The value for `key`, or `None` when it is missing or cannot be parsed as `T`.
    #[must_use]
    pub fn get<T: FromStr>(&self, key: &str) -> Option<T> {
        self.get_str(key)?.parse().ok()
    }

    /// The value for `key`, or `default` when it is missing or cannot be parsed.
    #[must_use]
    pub fn get_or<T: FromStr>(&self, key: &str, default: T) -> T {
        self.get(key).unwrap_or(default)
    }

    /// Stores `value` (its `Display` text) under `key`. Line breaks become spaces and `=`
    /// in keys becomes `_`, so every value reads back as one line.
    pub fn set(&mut self, key: &str, value: impl Display) {
        let key: String = key
            .trim()
            .chars()
            .map(|c| {
                if matches!(c, '=' | '\n' | '\r') {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let value: String = value
            .to_string()
            .chars()
            .map(|c| if matches!(c, '\n' | '\r') { ' ' } else { c })
            .collect();
        if !key.is_empty() {
            self.values.insert(key, value.trim().to_owned());
        }
    }

    /// Removes `key`; returns whether it was there.
    pub fn remove(&mut self, key: &str) -> bool {
        self.values.remove(key).is_some()
    }

    /// Whether `key` has a value.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    /// Every key, sorted.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// The bindings stored under `key` by [`SaveFile::set_bindings`]. Names that cannot be
    /// parsed are skipped; `None` when the key is missing or no binding is left.
    #[must_use]
    pub fn bindings(&self, key: &str) -> Option<Vec<Binding>> {
        let bindings: Vec<Binding> = self
            .get_str(key)?
            .split_whitespace()
            .filter_map(|name| name.parse().ok())
            .collect();
        (!bindings.is_empty()).then_some(bindings)
    }

    /// Stores `bindings` under `key`, separated by spaces (`Up w`).
    pub fn set_bindings(&mut self, key: &str, bindings: impl IntoIterator<Item = Binding>) {
        let names: Vec<String> = bindings.into_iter().map(|b| b.to_string()).collect();
        self.set(key, names.join(" "));
    }
}

/// The folder for the data of the game `app`, or `None` when the system does not say
/// where the user's home is:
///
/// - Windows: `%APPDATA%\app`
/// - macOS: `~/Library/Application Support/app`
/// - elsewhere: `$XDG_DATA_HOME/app`, or `~/.local/share/app`
///
/// Path separators in `app` are replaced by `_`.
#[must_use]
pub fn data_dir(app: &str) -> Option<PathBuf> {
    data_dir_for(std::env::consts::OS, app, |name| std::env::var_os(name))
}

fn data_dir_for(
    os: &str,
    app: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let app: String = app
        .chars()
        .map(|c| if matches!(c, '/' | '\\') { '_' } else { c })
        .collect();
    let absolute = |name: &str| {
        var(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = match os {
        "windows" => absolute("APPDATA"),
        "macos" => absolute("HOME").map(|home| home.join("Library/Application Support")),
        _ => absolute("XDG_DATA_HOME").or_else(|| absolute("HOME").map(|h| h.join(".local/share"))),
    }?;
    Some(base.join(app))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use vnce_term::KeyCode;
    use vnce_term::input::MouseButton;

    use super::*;

    /// A fresh folder in the system temp folder for one test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("vnce-save-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn parses_text() {
        let save = SaveFile::from_text(
            "# comment\n\n best = 10 \nbroken line\n=no key\nname = a = b\nbest = 12\r\n",
        );
        assert_eq!(save.get::<u32>("best"), Some(12));
        assert_eq!(save.get_str("name"), Some("a = b"));
        assert_eq!(save.keys().collect::<Vec<_>>(), ["best", "name"]);
    }

    #[test]
    fn text_round_trip() {
        let mut save = SaveFile::new();
        save.set("zeta", "last");
        save.set("alpha", 1.5);
        save.set("multi", "one\ntwo");
        save.set("a=b", true);
        let text = save.to_text();
        assert!(text.starts_with('#'));
        assert!(text.find("alpha").unwrap() < text.find("zeta").unwrap());
        let read = SaveFile::from_text(&text);
        assert_eq!(read.values, save.values);
        assert_eq!(read.get_str("multi"), Some("one two"));
        assert_eq!(read.get::<bool>("a_b"), Some(true));
    }

    #[test]
    fn typed_access() {
        let mut save = SaveFile::new();
        save.set("volume", 70);
        save.set("word", "hello");
        assert_eq!(save.get::<u8>("volume"), Some(70));
        assert_eq!(save.get::<u32>("word"), None);
        assert_eq!(save.get_or("word", 5), 5);
        assert!(save.contains("word"));
        assert!(save.remove("word"));
        assert!(!save.remove("word"));
        save.set("  ", 1);
        assert_eq!(save.keys().count(), 1);
    }

    #[test]
    fn bindings() {
        let mut save = SaveFile::new();
        let controls = [
            Binding::Key(KeyCode::Up),
            Binding::from('w'),
            Binding::from(' '),
            Binding::Mouse(MouseButton::Left),
        ];
        save.set_bindings("up", controls);
        assert_eq!(save.get_str("up"), Some("Up w Space mouse:left"));
        assert_eq!(save.bindings("up"), Some(controls.to_vec()));
        save.set("down", "nonsense S");
        assert_eq!(save.bindings("down"), Some(vec![Binding::from('S')]));
        save.set("left", "nonsense");
        assert_eq!(save.bindings("left"), None);
        assert_eq!(save.bindings("right"), None);
    }

    #[test]
    fn saves_and_loads_files() {
        let dir = temp_dir("files");
        let path = dir.join("nested").join("save.txt");
        let mut save = SaveFile::at(&path);
        assert_eq!(save.keys().count(), 0);
        save.set("best", 99);
        save.save().unwrap();
        save.set("best", 100);
        save.save().unwrap(); // overwrites
        let loaded = SaveFile::at(&path);
        assert_eq!(loaded.get::<u32>("best"), Some(100));
        assert_eq!(loaded.path(), Some(path.as_path()));
        assert!(!dir.join("nested").join("save.txt.tmp").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unreadable_files_start_empty() {
        let dir = temp_dir("unreadable");
        fs::create_dir_all(&dir).unwrap();
        // A folder cannot be read as a file.
        let save = SaveFile::at(&dir);
        assert_eq!(save.keys().count(), 0);
        fs::write(dir.join("binary.txt"), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(SaveFile::at(dir.join("binary.txt")).keys().count(), 0);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn memory_save_does_nothing() {
        assert_eq!(SaveFile::new().path(), None);
        SaveFile::new().save().unwrap();
    }

    /// Environment variables from a list.
    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn data_dirs() {
        #[cfg(windows)]
        let (home, appdata) = (r"C:\Users\me", r"C:\Users\me\AppData\Roaming");
        #[cfg(not(windows))]
        let (home, appdata) = ("/home/me", "/home/me/appdata");
        let pairs = [("HOME", home), ("APPDATA", appdata)];
        assert_eq!(
            data_dir_for("windows", "game", env(&pairs)),
            Some(PathBuf::from(appdata).join("game"))
        );
        assert_eq!(
            data_dir_for("macos", "game", env(&pairs)),
            Some(
                PathBuf::from(home)
                    .join("Library/Application Support")
                    .join("game")
            )
        );
        assert_eq!(
            data_dir_for("linux", "a/b", env(&pairs)),
            Some(PathBuf::from(home).join(".local/share").join("a_b"))
        );
        assert_eq!(data_dir_for("linux", "game", env(&[])), None);
        assert_eq!(
            data_dir_for("linux", "game", env(&[("HOME", "relative")])),
            None
        );
    }
}
