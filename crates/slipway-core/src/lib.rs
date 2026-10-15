//! Pure model of a signed, versioned release: the index (`release.toml`), versions, the signature
//! and digest checks, the install plan, the export-link plan, the daily-check schedule and the
//! start-up health decision.
//!
//! Nothing here touches the network, the bus or the disk. The caller reads bytes and observes the
//! file system; this crate says what is allowed and what to do next.

pub mod audit;
pub mod digest;
pub mod exports;
pub mod health;
pub mod path;
pub mod plan;
pub mod progress;
pub mod release;
pub mod schedule;
pub mod sign;
pub mod version;

pub use audit::{Audit, AuditError, EntryKind};
pub use digest::{Hasher, Sha256Hex};
pub use exports::{ExportAction, Found, plan_exports};
pub use health::{Boot, Pending, PendingStage, Restore, decide_boot};
pub use path::{BundlePath, PathError};
pub use plan::{Installed, PlanError, Step, plan_install, plan_rollback, prune};
pub use progress::{Emit, ProgressGate};
pub use release::{BundleRef, Export, FileEntry, Release, ReleaseError};
pub use schedule::{Attempt, Due, Outcome, UnixSeconds, next_check};
pub use sign::{Authenticator, PublicKey, SignError};
pub use version::{Standing, standing};
