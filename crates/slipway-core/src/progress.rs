//! How often download progress is announced.

/// The permille step that always earns an announcement.
const STEP_PERMILLE: u32 = 100;
/// The quietest the progress is allowed to be, in milliseconds.
const MAX_QUIET_MS: u64 = 250;
/// A finished download.
const DONE: u32 = 1000;

/// What to do with a progress reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Emit {
    /// Announce it.
    Announce,
    /// Keep it to ourselves.
    Skip,
}

/// Throttles progress: a reading is announced when it crosses a 100-permille step, when 250 ms
/// have passed since the last announcement, or when the download is done.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProgressGate {
    last: Option<(u32, u64)>,
}

impl ProgressGate {
    /// A gate that has announced nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Judges a reading of `permille` taken at `now_ms` (any steady clock), and remembers it when
    /// it is announced.
    pub fn admit(&mut self, permille: u32, now_ms: u64) -> Emit {
        let permille = permille.min(DONE);
        let announce = match self.last {
            None => true,
            Some((was, at)) => {
                permille == DONE && was != DONE
                    || permille / STEP_PERMILLE > was / STEP_PERMILLE
                    || (permille != was && now_ms.saturating_sub(at) >= MAX_QUIET_MS)
            }
        };
        if announce {
            self.last = Some((permille, now_ms));
            Emit::Announce
        } else {
            Emit::Skip
        }
    }
}
