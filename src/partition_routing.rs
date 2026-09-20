//! Safe bindings for the engine's distributed partition-routing hooks.
//!
//! Partitioned node tables keep their rows in partition subgraphs (`<parent>_p<i>`). In a
//! distributed deployment a partition may live on another host; the engine exposes one opaque
//! interface for that (C++ `common::PartitionRoutingHooks`, upstream PR #829) and stays
//! distribution-agnostic itself. This module binds it:
//!
//! - [`RoutingGuard::install`] registers process-global hooks. It must be called **before
//!   opening any Database**, exactly like the C++ contract.
//! - Placement ([`Callbacks::locate`]) decides per partition whether the wrapper owns it
//!   (remote) or the engine stores it locally. Unset means all-local.
//! - Lifecycle ([`Callbacks::on_partition_create`] / `on_partition_drop`) observes
//!   provisioning. Create fires for every partition, which is how a wrapper learns parent
//!   table IDs.
//! - Writes to claimed partitions ([`Callbacks::insert_row`]) arrive as decoded
//!   [`Value`] rows in parent schema order. Bulk writes (`COPY`, `INSERT ... SELECT`) fan
//!   out to the same callback row-at-a-time.
//! - Reads of claimed partitions are served by a bundled scan over the rows the wrapper
//!   accepted, so install + insert + read forms a complete remote loop with no further
//!   machinery. Call [`RoutingGuard::register_parent_schema`] with the parent's property
//!   names and types (scalar types only) before first read; without it, binding a scan of
//!   that parent fails instead of returning wrong results.
//!
//! `lookupRow` is intentionally unexposed: MERGE against claimed partitions keeps the
//! engine's default "not supported" behavior.
//!
//! Availability: this module is always built. It requires headers carrying the hooks
//! (prebuilt archives since ladybug #1005), which the C++ shim consumes from the
//! amalgamated `lbug.hpp` alone — no source checkout is needed.
//!
//! # Threading and lifetime
//!
//! The engine may invoke callbacks from any query thread; every callback must be `Send +
//! Sync`. Callbacks must not panic and must not call back into routing management
//! ([`RoutingGuard::install`] / [`uninstall`](RoutingGuard::uninstall) /
//! [`register_parent_schema`](RoutingGuard::register_parent_schema)); a panicking callback
//! aborts the process rather than letting corrupt state flow back into the engine.
//!
//! Uninstall (dropping the guard) resets engine hooks to null. Drop every [`Database`](crate::Database)
//! first: the engine consults hooks during checkpointing, and storage claimed as remote has
//! no local files to fall back to.

use std::ffi::{c_char, c_void, CString};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::error::Error;
use crate::ffi::ffi;
use crate::logical_type::LogicalType;
use crate::value::Value;

/// Identifies one partition subgraph: the partitioned parent's table ID plus the partition
/// index. Mirrors `lbug::common::PartitionRef`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PartitionRef {
    pub parent_table_id: u64,
    pub partition_index: u64,
}

/// Wrapper callbacks implementing the distributed side of partitioned tables.
///
/// Every member is optional; unset means "handle locally" (for [`Callbacks::locate`]) or
/// "not observed" (everything else), preserving embedded behavior bit-for-bit.
#[derive(Default)]
pub struct Callbacks {
    /// Placement: return `Some(handle)` to claim the partition as remote, `None` for local
    /// storage. The handle is an opaque word handed back verbatim on every later callback
    /// for that partition. Must be side-effect free and consistent.
    pub locate: Option<Box<dyn Fn(PartitionRef) -> Option<u64> + Send + Sync>>,
    /// Fires for every partition creation (how a wrapper learns parent table IDs).
    pub on_partition_create: Option<Box<dyn Fn(PartitionRef) + Send + Sync>>,
    /// Fires for claimed partitions when their subgraph entry is dropped.
    pub on_partition_drop: Option<Box<dyn Fn(PartitionRef) + Send + Sync>>,
    /// One call per row routed to a claimed partition, cells in parent schema order.
    /// Observe the row here (replicate, log, assert); the bundled row store assigns the
    /// node ID the engine uses.
    pub insert_row: Option<Box<dyn Fn(PartitionRef, Vec<Value>) + Send + Sync>>,
}

/// Process-global routing installation. Drop (or [`uninstall`](RoutingGuard::uninstall)) to
/// reset engine hooks to null. See the [module docs](self) for ordering rules.
pub struct RoutingGuard {
    _private: (),
}

struct State {
    callbacks: Callbacks,
}

static INSTALLED: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<Box<State>>> = Mutex::new(None);

#[repr(C)]
struct CPartitionRef {
    parent_table_id: u64,
    partition_index: u64,
}

impl From<CPartitionRef> for PartitionRef {
    fn from(r: CPartitionRef) -> Self {
        PartitionRef {
            parent_table_id: r.parent_table_id,
            partition_index: r.partition_index,
        }
    }
}

#[repr(C)]
struct CHooks {
    context: *mut c_void,
    locate: Option<extern "C" fn(*mut c_void, CPartitionRef, *mut *mut c_void) -> u8>,
    on_partition_create: Option<extern "C" fn(*mut c_void, CPartitionRef, *mut c_void)>,
    on_partition_drop: Option<extern "C" fn(*mut c_void, CPartitionRef, *mut c_void)>,
    insert_row:
        Option<extern "C" fn(*mut c_void, CPartitionRef, *mut c_void, *const *const c_void, usize)>,
}

extern "C" {
    fn lbug_partition_routing_install(hooks: *const CHooks) -> libc_int_t;
    fn lbug_partition_routing_uninstall();
    fn lbug_partition_routing_is_installed() -> u8;
    fn lbug_partition_routing_register_schema(
        parent_table_id: u64,
        prop_names: *const *const c_char,
        type_ids: *const u8,
        n_props: usize,
    ) -> libc_int_t;
}

fn with_state<F, R>(context: *mut c_void, f: F) -> R
where
    F: FnOnce(&State) -> R,
{
    // SAFETY: `context` is the `Box<State>` address captured at install; the box lives in
    // the process-global slot until uninstall, and uninstall requires no live queries.
    let state = unsafe { &*(context as *const State) };
    f(state)
}

fn abort_on_panic(context: &str, result: std::thread::Result<()>) {
    if let Err(payload) = result {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "unknown panic".to_string());
        eprintln!("ladybug routing callback panicked in {context}: {message}; aborting");
        std::process::abort();
    }
}

extern "C" fn locate_cb(
    context: *mut c_void,
    pref: CPartitionRef,
    handle_out: *mut *mut c_void,
) -> u8 {
    let result = std::panic::catch_unwind(|| {
        with_state(context, |state| {
            state
                .callbacks
                .locate
                .as_ref()
                .and_then(|locate| locate(pref.into()))
        })
    });
    match result {
        Ok(Some(handle)) => {
            // SAFETY: the engine only hands the word back verbatim.
            unsafe { *handle_out = handle as *mut c_void };
            1
        }
        Ok(None) | Err(_) => 0,
    }
}

extern "C" fn create_cb(context: *mut c_void, pref: CPartitionRef, _handle: *mut c_void) {
    let result = std::panic::catch_unwind(|| {
        with_state(context, |state| {
            if let Some(cb) = state.callbacks.on_partition_create.as_ref() {
                cb(pref.into());
            }
        });
    });
    abort_on_panic("on_partition_create", result);
}

extern "C" fn drop_cb(context: *mut c_void, pref: CPartitionRef, _handle: *mut c_void) {
    let result = std::panic::catch_unwind(|| {
        with_state(context, |state| {
            if let Some(cb) = state.callbacks.on_partition_drop.as_ref() {
                cb(pref.into());
            }
        });
    });
    abort_on_panic("on_partition_drop", result);
}

extern "C" fn insert_row_cb(
    context: *mut c_void,
    pref: CPartitionRef,
    _handle: *mut c_void,
    cells: *const *const c_void,
    n_cells: usize,
) {
    let result = std::panic::catch_unwind(|| {
        with_state(context, |state| {
            let Some(cb) = state.callbacks.insert_row.as_ref() else {
                return;
            };
            let mut row = Vec::with_capacity(n_cells);
            for i in 0..n_cells {
                // SAFETY: the shim guarantees `cells[i]` borrows a live
                // `lbug::common::Value` for the duration of this call.
                let cell = unsafe { &*(*cells.add(i)).cast::<ffi::Value>() };
                match Value::try_from(cell) {
                    Ok(value) => row.push(value),
                    Err(e) => {
                        eprintln!("ladybug routing: cannot decode routed cell {i}: {e}; aborting");
                        std::process::abort();
                    }
                }
            }
            cb(pref.into(), row);
        });
    });
    abort_on_panic("insert_row", result);
}

// Discriminants of `lbug::common::LogicalTypeID` (see `ffi::LogicalTypeID`, which mirrors
// `common/types/types.h`). Spelled out because cxx marks explicitly-discriminated enums
// non-exhaustive, which forbids `as` casts; keep in sync with the bridge definition.
fn logical_type_id(logical_type: &LogicalType) -> Result<u8, Error> {
    match logical_type {
        LogicalType::Any => Ok(0),
        LogicalType::Bool => Ok(22),
        LogicalType::Serial => Ok(13),
        LogicalType::Int64 => Ok(23),
        LogicalType::Int32 => Ok(24),
        LogicalType::Int16 => Ok(25),
        LogicalType::Int8 => Ok(26),
        LogicalType::UInt64 => Ok(27),
        LogicalType::UInt32 => Ok(28),
        LogicalType::UInt16 => Ok(29),
        LogicalType::UInt8 => Ok(30),
        LogicalType::Int128 => Ok(31),
        LogicalType::Double => Ok(32),
        LogicalType::Float => Ok(33),
        LogicalType::Date => Ok(34),
        LogicalType::Timestamp => Ok(35),
        LogicalType::TimestampTz => Ok(39),
        LogicalType::TimestampNs => Ok(38),
        LogicalType::TimestampMs => Ok(37),
        LogicalType::TimestampSec => Ok(36),
        LogicalType::String => Ok(50),
        LogicalType::UUID => Ok(59),
        LogicalType::Json => Ok(60),
        other => Err(Error::FailedQuery(format!(
            "partition routing schemas support scalar types only, got {other:?}"
        ))),
    }
}

impl RoutingGuard {
    /// Install process-global routing hooks. Must be called before opening any Database;
    /// installing twice without [`uninstall`](RoutingGuard::uninstall) is an error.
    pub fn install(callbacks: Callbacks) -> Result<Self, Error> {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return Err(Error::FailedQuery(
                "partition routing hooks are already installed".to_string(),
            ));
        }
        let mut slot = STATE.lock().unwrap();
        let state = Box::new(State { callbacks });
        let context: *mut c_void = std::ptr::from_ref(&*state).cast_mut().cast();
        let c_hooks = CHooks {
            context,
            locate: Some(locate_cb),
            on_partition_create: Some(create_cb),
            on_partition_drop: Some(drop_cb),
            insert_row: Some(insert_row_cb),
        };
        // SAFETY: the shim copies the struct; `context` stays valid in the global slot
        // until uninstall, which requires no live queries.
        let rc = unsafe { lbug_partition_routing_install(std::ptr::from_ref(&c_hooks)) };
        if rc != 0 {
            INSTALLED.store(false, Ordering::SeqCst);
            return Err(Error::FailedQuery(
                "engine rejected partition routing installation".to_string(),
            ));
        }
        *slot = Some(state);
        Ok(RoutingGuard { _private: () })
    }

    /// Register a parent table's scan schema so its claimed partitions read back through
    /// the bundled scan. `columns` are the parent's properties in schema order. Required
    /// before the first read of a claimed partition of that parent.
    pub fn register_parent_schema(
        &self,
        parent_table_id: u64,
        columns: Vec<(String, LogicalType)>,
    ) -> Result<(), Error> {
        if columns.is_empty() {
            return Err(Error::FailedQuery(
                "partition routing schema needs at least one column".to_string(),
            ));
        }
        let names: Vec<CString> = columns
            .iter()
            .map(|(name, _)| {
                CString::new(name.as_str()).map_err(|_| {
                    Error::FailedQuery(format!("property name {name:?} contains a NUL byte"))
                })
            })
            .collect::<Result<_, _>>()?;
        let name_ptrs: Vec<*const c_char> = names.iter().map(|n| n.as_ptr()).collect();
        let type_ids: Vec<u8> = columns
            .iter()
            .map(|(_, typ)| logical_type_id(typ))
            .collect::<Result<_, _>>()?;
        // SAFETY: pointers borrow live locals for a synchronous call.
        let rc = unsafe {
            lbug_partition_routing_register_schema(
                parent_table_id,
                name_ptrs.as_ptr(),
                type_ids.as_ptr(),
                columns.len(),
            )
        };
        if rc != 0 {
            return Err(Error::FailedQuery(
                "engine rejected partition routing schema".to_string(),
            ));
        }
        Ok(())
    }

    /// Returns true while engine hooks are installed.
    pub fn is_installed(&self) -> bool {
        // SAFETY: pure query, no state touched.
        unsafe { lbug_partition_routing_is_installed() != 0 }
    }

    /// Reset engine hooks to null. Drop every Database first (see module docs).
    pub fn uninstall(self) {}
}

impl Drop for RoutingGuard {
    fn drop(&mut self) {
        // SAFETY: no engine state is touched beyond resetting the global pointer; callers
        // must have dropped all Databases (see module docs).
        unsafe { lbug_partition_routing_uninstall() };
        INSTALLED.store(false, Ordering::SeqCst);
        *STATE.lock().unwrap() = None;
    }
}

// `libc` is not a direct dependency; `std::os::raw::c_int` is the same C int.
#[allow(non_camel_case_types)]
type libc_int_t = std::os::raw::c_int;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::Connection;
    use crate::database::{Database, SystemConfig};
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    #[test]
    fn remote_partition_round_trip() {
        // NOTE: hooks are process-global and cargo runs tests in threads of one
        // binary, so every hook assertion lives in this single test. No other test
        // in this crate creates partitioned tables, which is all the hooks observe.
        // The guard is declared first so it drops last, after db and conn.
        let created: Arc<Mutex<Vec<PartitionRef>>> = Arc::new(Mutex::new(Vec::new()));
        let dropped: Arc<Mutex<Vec<PartitionRef>>> = Arc::new(Mutex::new(Vec::new()));
        let observed: Arc<Mutex<Vec<(PartitionRef, Vec<Value>)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let created_cb = created.clone();
        let dropped_cb = dropped.clone();
        let observed_cb = observed.clone();
        // Double install fails while the first guard lives.
        let guard = RoutingGuard::install(Callbacks::default()).unwrap();
        assert!(
            RoutingGuard::install(Callbacks::default()).is_err(),
            "double install must fail"
        );
        drop(guard);

        let guard = RoutingGuard::install(Callbacks {
            // Claim every partition of the test table (fully-remote parents scan
            // through one substitute entry; mixed local/remote scans are rejected).
            // NOTE: HASH parents only. Claiming a LIST partition is an engine gap as
            // of ladybug 0.20: dynamic LIST creation throws `unordered_map::at` instead
            // of routing remotely (the HASH path returns a null table and branches to
            // insertRow; see `getOrCreatePartitionLocked`).
            locate: Some(Box::new(|_r: PartitionRef| Some(0xC0FFEE))),
            on_partition_create: Some(Box::new(move |r: PartitionRef| {
                created_cb.lock().unwrap().push(r);
            })),
            on_partition_drop: Some(Box::new(move |r: PartitionRef| {
                dropped_cb.lock().unwrap().push(r);
            })),
            insert_row: Some(Box::new(move |r: PartitionRef, row: Vec<Value>| {
                observed_cb.lock().unwrap().push((r, row));
            })),
        })
        .unwrap();
        assert!(guard.is_installed());

        // A file database in a temp dir: per-partition data files land next to it and
        // vanish with the dir (an in-memory database spills them into the cwd instead).
        let db_dir = tempfile::tempdir().unwrap();
        let db = Database::new(db_dir.path().join("routing"), SystemConfig::default()).unwrap();
        let conn = Connection::new(&db).unwrap();
        conn.query(
            "CREATE NODE TABLE Remote(id INT64, v INT64, PRIMARY KEY(id)) PARTITION BY HASH(v) PARTITIONS 3;",
        )
        .unwrap();

        // The parent table ID arrives via the create notifications.
        let parent = created.lock().unwrap()[0].parent_table_id;
        guard
            .register_parent_schema(
                parent,
                vec![
                    ("id".to_string(), LogicalType::Int64),
                    ("v".to_string(), LogicalType::Int64),
                ],
            )
            .unwrap();

        // Point writes route through insert_row into the remote store.
        conn.query("CREATE (:Remote {id: 1, v: 10});").unwrap();
        conn.query("CREATE (:Remote {id: 2, v: 10});").unwrap();
        conn.query("CREATE (:Remote {id: 3, v: 20});").unwrap();

        // Bulk writes route through insertChunk, fanned out to the same callback.
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("bulk.csv");
        std::fs::write(&csv, "4,30\n5,30\n").unwrap();
        conn.query(&format!(
            "COPY Remote FROM '{}';",
            csv.to_string_lossy().replace('\\', "/")
        ))
        .unwrap();

        // Reads come back through the bundled scan, per partition and via the parent union.
        let result = conn
            .query("MATCH (r:Remote) RETURN r.id, r.v ORDER BY r.id;")
            .unwrap()
            .to_string();
        assert_eq!(result, "r.id|r.v\n1|10\n2|10\n3|20\n4|30\n5|30\n");

        let seen: HashSet<i64> = observed
            .lock()
            .unwrap()
            .iter()
            .map(|(_, row)| match &row[0] {
                Value::Int64(id) => *id,
                other => panic!("expected Int64 id, got {other:?}"),
            })
            .collect();
        assert_eq!(seen, HashSet::from([1, 2, 3, 4, 5]));
        assert_eq!(
            created.lock().unwrap().len(),
            3,
            "one create per HASH partition"
        );

        // Dropping the parent notifies once per claimed partition.
        conn.query("DROP TABLE Remote;").unwrap();
        assert_eq!(dropped.lock().unwrap().len(), 3);
        drop(conn);
        drop(db);
        drop(dir);
        db_dir.close().unwrap();
        guard.uninstall();
    }
}
