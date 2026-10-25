//! How a session shows its results. Display reads the running session's, so outside one it sees the defaults.

#[derive(Clone, Debug)]
pub struct Settings {
    /// Significant digits in a float, 1 to 17.
    pub precision: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { precision: 6 }
    }
}
