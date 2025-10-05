pub mod clipboard;
pub mod repo_file;
pub mod repo_mem;
pub mod terminals;

pub use clipboard::{default_clipboard_backends, CommandExecutor, SystemCommandExecutor};
pub use repo_file::FileTermBridgeStateRepository;
#[allow(unused_imports)]
pub use repo_mem::{InMemoryTermBridgeBindingRepository, InMemoryTermBridgeStateRepository};
pub use terminals::{
    AlacrittyAdapter, ITerm2Adapter, KittyAdapter, KonsoleAdapter, TilixAdapter, WezTermAdapter,
    WindowsTerminalAdapter,
};
