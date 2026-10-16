//! Pure model of a signed, versioned release: the index (`release.toml`), versions, the signature
//! and digest checks, the install plan, the export-link plan, the daily-check schedule and the
//! start-up health decision.
//!
//! Nothing here touches the network, the bus or the disk. The caller reads bytes and observes the
//! file system; this crate says what is allowed and what to do next.

pub mod audit;
pub mod digest;
pub mod exports;
pub mod guard;
pub mod health;
pub mod path;
pub mod plan;
pub mod progress;
pub mod release;
pub mod schedule;
pub mod sign;
pub mod system;
pub mod version;

pub use audit::{Audit, AuditError, EntryKind};
pub use digest::{Hasher, Sha256Hex};
pub use exports::{ExportAction, Found, plan_exports};
pub use guard::{Guard, MAX_UNCONFIRMED_BOOTS, Record as GuardRecord, Trust};
pub use health::{Boot, LoginSession, Pending, PendingStage, Restore, decide_boot};
pub use path::{BundlePath, PathError};
pub use plan::{Installed, PlanError, Step, plan_install, plan_rollback, prune};
pub use progress::{Emit, ProgressGate};
pub use release::{BundleRef, Export, FileEntry, Release, ReleaseError, SystemFile};
pub use schedule::{Attempt, Due, Outcome, UnixSeconds, next_check};
pub use sign::{Authenticator, PublicKey, SignError};
pub use system::{Placement, SystemAction, SystemFound, placements, plan_system, vendor_copies};
pub use version::{Offer, Standing, offer, standing};
