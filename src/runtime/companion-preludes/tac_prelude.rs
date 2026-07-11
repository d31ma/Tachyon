// Tachyon editor prelude for Rust Tac companions.
//
// Never compiled or bundled by Tachyon (only `tac.rs` beside a `tac.html` is a
// companion). Copy this file next to your companion so rust-analyzer resolves
// the implicit prelude symbols.
//
// ponytail: rustc still rejects Tac attributes (#[onMount], #[publish("...")])
// because they would need a proc-macro crate; this stub only fixes symbol
// resolution. Rust companions are a Tac dialect, not real Rust — see the
// portable companion subset reference in the Tachyon README.

pub struct Storage;
impl Storage {
    pub fn get_item(&self, _key: &str, _fallback: &str) -> String { String::new() }
    pub fn set_item(&self, _key: &str, _value: &str) {}
    pub fn remove_item(&self, _key: &str) {}
}
pub fn local_storage() -> Storage { Storage }
pub fn session_storage() -> Storage { Storage }

pub struct Navigator;
impl Navigator {
    pub fn language(&self) -> String { String::new() }
    pub fn is_online(&self) -> bool { false }
}
pub fn navigator() -> Navigator { Navigator }

pub struct Location;
impl Location {
    pub fn href(&self) -> String { String::new() }
    pub fn origin(&self) -> String { String::new() }
}
pub fn location() -> Location { Location }

pub struct FyloCollection;
impl FyloCollection {
    pub fn find(&self, _query: ()) {}
    pub fn get(&self, _id: &str) {}
    pub fn create(&self, _document: ()) {}
    pub fn put(&self, _document: ()) {}
    pub fn delete(&self, _id: &str) {}
}
pub struct Fylo;
impl Fylo {
    pub fn collection(&self, _name: &str) -> FyloCollection { FyloCollection }
}
pub fn fylo() -> Fylo { Fylo }

pub struct AppInfo { pub name: String, pub version: String }
pub struct App;
impl App {
    pub fn is_available(&self) -> bool { false }
    pub fn info(&self) -> AppInfo { AppInfo { name: String::new(), version: String::new() } }
}
pub fn app() -> App { App }

pub struct Clipboard;
impl Clipboard {
    pub fn read_text(&self) -> String { String::new() }
    pub fn write_text(&self, _text: &str) {}
}
pub fn clipboard() -> Clipboard { Clipboard }

pub struct FileSystem;
impl FileSystem {
    pub fn read_text(&self, _path: &str) -> String { String::new() }
    pub fn write_text(&self, _path: &str, _text: &str) {}
    pub fn read_dir(&self, _path: &str) {}
    pub fn paths(&self) {}
}
pub fn file_system() -> FileSystem { FileSystem }

pub struct Shell;
impl Shell {
    pub fn exec(&self, _command: &str, _args: &[&str]) {}
}
pub fn shell() -> Shell { Shell }

pub struct Browser;
impl Browser {
    pub fn open(&self, _url: &str) {}
}
pub fn browser() -> Browser { Browser }

pub struct Share;
impl Share {
    pub fn text(&self, _text: &str) {}
}
pub fn share() -> Share { Share }

pub struct Haptics;
impl Haptics {
    pub fn impact(&self) {}
}
pub fn haptics() -> Haptics { Haptics }

pub struct FilePicker;
impl FilePicker {
    pub fn open_text(&self) -> String { String::new() }
    pub fn save_text(&self, _name: &str, _text: &str) {}
}
pub fn file_picker() -> FilePicker { FilePicker }

pub struct Capabilities;
impl Capabilities {
    pub fn supports(&self, _capability: &str) -> bool { false }
    pub fn state(&self, _capability: &str) -> String { String::new() }
}
pub fn capabilities() -> Capabilities { Capabilities }

pub struct Secrets;
impl Secrets { pub fn get(&self, _key: &str) -> Option<String> { None } pub fn set(&self, _key: &str, _value: &str) {} pub fn delete(&self, _key: &str) {} }
pub fn secrets() -> Secrets { Secrets }
pub struct Auth;
impl Auth { pub fn verify_user(&self, _reason: &str) {} }
pub fn auth() -> Auth { Auth }
pub struct Geolocation;
impl Geolocation { pub fn current(&self) {} }
pub fn geolocation() -> Geolocation { Geolocation }
pub struct Notifications;
impl Notifications { pub fn show(&self, _title: &str) {} }
pub fn notifications() -> Notifications { Notifications }
pub struct Media;
impl Media { pub fn get_user_media(&self, _constraints: ()) {} }
pub fn media() -> Media { Media }
