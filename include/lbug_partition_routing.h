// C ABI for LadybugDB partition routing hooks.
//
// The engine's `common::PartitionRoutingHooks` struct is not FFI-safe (it carries
// `std::function` members and callbacks with C++-only signatures such as
// `transaction::Transaction*` and `const ValueVector*`). This header exposes the same
// contract through plain function pointers, implemented in
// `src/lbug_partition_routing.cpp`:
//
// - `locate` / `on_partition_create` / `on_partition_drop` map one-to-one to the engine
//   hooks. A null function pointer means "not handled" (local storage for `locate`).
// - `insert_row` fires once per routed row (point writes call it directly; bulk writes fan
//   out to it row-at-a-time in the shim). Cells arrive in parent schema order as borrowed
//   `lbug::common::Value` objects, valid only for the duration of the call. The shim owns a
//   row store per claimed partition and assigns node IDs from it; the callback observes
//   rows (e.g. to replicate or assert them).
// - Reads of claimed partitions are served by a bundled scan function over that same row
//   store; see `lbug_partition_routing_register_schema`. `lookupRow` is intentionally left
//   unset, so MERGE against claimed partitions fails with the engine's "not supported"
//   error, exactly as with no wrapper at all.

#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Mirrors `lbug::common::PartitionRef`: (partitioned parent table ID, partition index).
typedef struct {
    uint64_t parent_table_id;
    uint64_t partition_index;
} lbug_partition_ref_t;

// Mirrors the engine hooks with C-compatible signatures. `context` is passed back verbatim.
typedef struct {
    void* context;
    // Return 1 + set *handle_out to claim the partition as remote, 0 for local storage.
    uint8_t (*locate)(void* context, lbug_partition_ref_t ref, void** handle_out);
    void (*on_partition_create)(void* context, lbug_partition_ref_t ref, void* handle);
    void (*on_partition_drop)(void* context, lbug_partition_ref_t ref, void* handle);
    // One call per routed row. `cells` borrows `n_cells` values, valid for the call only.
    void (*insert_row)(void* context, lbug_partition_ref_t ref, void* handle,
        const void* const* cells, size_t n_cells);
} lbug_partition_hooks_t;

// Install process-global routing hooks. Must be called before opening any Database.
// Returns 0 on success, 1 if hooks are already installed (reset with
// `lbug_partition_routing_uninstall` first). The pointed-to struct is copied.
int lbug_partition_routing_install(const lbug_partition_hooks_t* hooks);

// Reset engine hooks to null (local behavior). Existing Database objects must be dropped
// first; the registry keeps no state of its own beyond the installed flag.
void lbug_partition_routing_uninstall(void);

// Returns 1 while hooks are installed, 0 otherwise.
uint8_t lbug_partition_routing_is_installed(void);

// Register a parent table's scan schema so claimed partitions can be read back through the
// bundled scan function: `prop_names[i]` / `type_ids[i]` describe the parent's properties in
// schema order (`type_ids` are `lbug::common::LogicalTypeID` discriminants; scalar types
// only). Returns 0 on success, 1 on an unsupported type or a null argument.
int lbug_partition_routing_register_schema(uint64_t parent_table_id,
    const char* const* prop_names, const uint8_t* type_ids, size_t n_props);

#ifdef __cplusplus
}
#endif
