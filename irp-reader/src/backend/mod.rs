mod file;
#[cfg(feature = "windows-mmap")]
mod windows;

pub use file::FileReplaySource;
#[cfg(feature = "windows-mmap")]
pub use windows::WindowsMmapSource;
