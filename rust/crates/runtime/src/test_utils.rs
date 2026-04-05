use std::path::PathBuf;

#[derive(Debug)]
pub struct CwdGuard(PathBuf);

impl CwdGuard {
    #[must_use]
    pub fn new() -> Self {
        Self(std::env::current_dir().unwrap())
    }
}

impl Default for CwdGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).ok();
    }
}
