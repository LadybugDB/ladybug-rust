// C++ side of the partition routing FFI (see `include/lbug_partition_routing.h`).
//
// The engine calls the static forwarders below, which translate C++ callback arguments into
// the C ABI and invoke the wrapper's function pointers. Rows routed to claimed partitions
// land in a per-partition row store owned here; the bundled scan `TableFunction` serves
// them back, so a wrapper gets a complete remote loop (locate -> insert -> scan) without
// implementing table-function machinery itself.

#include "lbug_partition_routing.h"

#include <algorithm>
#include <cstring>
#include <map>
#include <memory>
#include <mutex>
#include <shared_mutex>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

// Everything engine-side comes from the amalgamated header, so this shim builds
// against prebuilt archives with no source checkout: ladybug #1005 ships the hook
// declarations plus the scan machinery (TableFunction, bind data, morsels, vectors)
// inside lbug.hpp. TableFuncBindInput stays incomplete there, so unlike the engine's
// own mock the bind callback builds columns directly instead of via Binder.
#include <lbug.hpp>

namespace {

using lbug::common::LogicalType;
using lbug::common::LogicalTypeID;
using lbug::common::Value;
using lbug::common::ValueVector;
using lbug::function::TableFuncBindData;
using lbug::function::TableFuncBindInput;
using lbug::function::TableFuncInput;
using lbug::function::TableFuncMorsel;
using lbug::function::TableFunction;

struct ParentSchema {
    // LogicalType is neither copyable nor default-constructible here, so the schema keeps
    // type IDs and builds fresh LogicalTypes where needed.
    std::vector<std::string> propNames;
    std::vector<LogicalTypeID> propTypeIDs;
};

struct Registry {
    std::shared_mutex mutex;
    lbug_partition_hooks_t callbacks{};
    // parentTableID -> every row accepted for any claimed partition of that parent, in
    // insert order (key column included). Reads collapse to one substitute entry per
    // parent, so the scan serves the concatenated content; see below.
    std::map<uint64_t, std::vector<std::vector<Value>>> rows;
    std::map<uint64_t, ParentSchema> schemas;
    // One scan function per parent table; heap-stable for the engine to borrow. The
    // engine rejects a parent scan whose partitions expose different scans (it reads as
    // a local/remote mix), so all claimed partitions of a parent MUST share one
    // function. That rules out per-partition kernels keyed off bind data, which the
    // engine copies through the base `TableFuncBindData` copy constructor anyway
    // (slicing away any derived identity fields).
    std::map<uint64_t, std::unique_ptr<TableFunction>> scanFunctions;
};

Registry& registry() {
    static Registry instance;
    return instance;
}

lbug_partition_ref_t toCRef(lbug::common::PartitionRef ref) {
    return lbug_partition_ref_t{ref.parentTableID, ref.partitionIndex};
}

// Materialize one evaluated row (selection position `selPos` of every vector) into owned
// Values. Null vectors (unprojected columns) become untyped nulls.
std::vector<Value> materializeRow(std::span<ValueVector* const> vectors, uint32_t selPos) {
    std::vector<Value> cells;
    cells.reserve(vectors.size());
    for (const ValueVector* vec : vectors) {
        if (vec == nullptr || vec->isNull(selPos)) {
            cells.push_back(Value::createNullValue());
        } else {
            cells.push_back(*vec->getAsValue(selPos));
        }
    }
    return cells;
}

// Borrowed pointers into `cells`, valid while `cells` is alive (i.e. for the callback).
std::vector<const void*> borrowCells(const std::vector<Value>& cells) {
    std::vector<const void*> ptrs;
    ptrs.reserve(cells.size());
    for (const Value& cell : cells) {
        ptrs.push_back(static_cast<const void*>(&cell));
    }
    return ptrs;
}

// --- Engine hook forwarders -------------------------------------------------

bool locateForward(void* context, lbug::common::PartitionRef ref, void** handleOut) {
    const auto& cbs = registry().callbacks;
    if (cbs.locate == nullptr) {
        return false;
    }
    return cbs.locate(context, toCRef(ref), handleOut) != 0;
}

void createForward(void* context, lbug::common::PartitionRef ref, void* handle) {
    const auto& cbs = registry().callbacks;
    if (cbs.on_partition_create != nullptr) {
        cbs.on_partition_create(context, toCRef(ref), handle);
    }
}

void dropForward(void* context, lbug::common::PartitionRef ref, void* handle) {
    const auto& cbs = registry().callbacks;
    if (cbs.on_partition_drop != nullptr) {
        cbs.on_partition_drop(context, toCRef(ref), handle);
    }
}

// Appends `cells` to the partition store and forwards them to the wrapper. Returns the
// assigned node ID (store position + parent table ID, mirroring the engine's own mock).
lbug::common::nodeID_t storeAndForward(lbug::common::PartitionRef ref, void* handle,
    std::vector<Value> cells) {
    auto& reg = registry();
    // Snapshot the stored row under lock: another writer's push_back may reallocate the
    // store, so the callback must borrow the copy, never the live vector.
    uint64_t offset = 0;
    // Held by pointer: Value has no copy assignment, so neither the row snapshot nor
    // the schema below may ever be copy-assigned, only copy/move-constructed.
    std::unique_ptr<std::vector<Value>> snapshot;
    {
        std::unique_lock lock(reg.mutex);
        auto& store = reg.rows[ref.parentTableID];
        offset = store.size();
        store.push_back(std::move(cells));
        snapshot = std::make_unique<std::vector<Value>>(store.back());
    }
    const auto ptrs = borrowCells(*snapshot);
    const auto& cbs = reg.callbacks;
    if (cbs.insert_row != nullptr) {
        cbs.insert_row(reg.callbacks.context, toCRef(ref), handle, ptrs.data(), ptrs.size());
    }
    return lbug::common::nodeID_t{static_cast<lbug::common::offset_t>(offset),
        ref.parentTableID};
}

lbug::common::nodeID_t insertRowForward(void* context, lbug::common::PartitionRef ref,
    void* handle, lbug::transaction::Transaction* /*tx*/, const ValueVector* keyVector,
    std::span<ValueVector* const> columnVectors) {
    (void)context;
    (void)keyVector;
    // `columnVectors` already holds every property in schema order (the key vector aliases
    // one of them); the single row sits at selection position 0.
    const uint32_t selPos = columnVectors.empty() ?
        0 :
        columnVectors[0]->state->getSelVector()[0];
    return storeAndForward(ref, handle, materializeRow(columnVectors, selPos));
}

void insertChunkForward(void* context, lbug::common::PartitionRef ref, void* handle,
    lbug::transaction::Transaction* /*tx*/, const ValueVector* keyVector,
    std::span<ValueVector* const> columnVectors, uint64_t startRow, uint64_t numRows) {
    (void)context;
    (void)keyVector;
    const auto& sel = columnVectors[0]->state->getSelVector();
    for (uint64_t j = 0; j < numRows; ++j) {
        storeAndForward(ref, handle, materializeRow(columnVectors, sel[startRow + j]));
    }
}

// --- Scan serving -----------------------------------------------------------

lbug::binder::expression_vector scanColumns(
    const ParentSchema& schema, const std::string& nodeUniqueName) {
    lbug::binder::expression_vector columns;
    columns.push_back(std::make_shared<lbug::binder::VariableExpression>(
        LogicalType(LogicalTypeID::INT64), nodeUniqueName + "._ID", "rowid"));
    for (size_t i = 0; i < schema.propNames.size(); ++i) {
        columns.push_back(std::make_shared<lbug::binder::VariableExpression>(
            LogicalType(schema.propTypeIDs[i]), nodeUniqueName + "." + schema.propNames[i],
            schema.propNames[i]));
    }
    return columns;
}

// Current row count of one parent store (bind-time snapshot for scan sizing).
lbug::common::row_idx_t storeSize(uint64_t parentTableID) {
    auto& reg = registry();
    std::shared_lock lock(reg.mutex);
    const auto it = reg.rows.find(parentTableID);
    return it == reg.rows.end() ?
        0 :
        static_cast<lbug::common::row_idx_t>(it->second.size());
}

// Serves rows [startOffset, startOffset + morsel) of one parent store: column 0 is the
// row id, the rest are the stored properties in schema order. Columns beyond the stored
// row width read null, so a wider bind never exposes garbage.
lbug::common::offset_t scanParent(
    uint64_t parentTableID, const TableFuncMorsel& morsel, lbug::common::DataChunk& output) {
    if (!morsel.hasMoreToOutput()) {
        return 0;
    }
    auto& reg = registry();
    std::vector<std::vector<Value>> slice;
    uint64_t start = 0;
    uint64_t count = 0;
    {
        std::shared_lock lock(reg.mutex);
        const auto it = reg.rows.find(parentTableID);
        const uint64_t storeSize =
            (it == reg.rows.end()) ? 0 : static_cast<uint64_t>(it->second.size());
        start = static_cast<uint64_t>(morsel.startOffset);
        if (start >= storeSize) {
            return 0;
        }
        count = std::min(morsel.getMorselSize(), storeSize - start);
        // Range-construct (never assign): Value has no copy assignment.
        std::vector<std::vector<Value>> fresh(
            it->second.begin() + start, it->second.begin() + start + count);
        slice.swap(fresh);
    }
    const uint64_t numOutputCols = output.getNumValueVectors();
    for (uint64_t i = 0; i < count; ++i) {
        output.getValueVectorMutable(0).copyFromValue(
            i, Value(static_cast<int64_t>(start + i)));
        const auto& row = slice[i];
        for (uint64_t c = 1; c < numOutputCols; ++c) {
            auto& vec = output.getValueVectorMutable(c);
            if (c - 1 >= row.size() || row[c - 1].isNull()) {
                vec.setNull(static_cast<uint32_t>(i), true);
            } else {
                vec.copyFromValue(i, row[c - 1]);
            }
        }
    }
    return static_cast<lbug::common::offset_t>(count);
}

bool bindScanForward(void* context, lbug::common::PartitionRef ref, void* /*handle*/,
    lbug::common::PartitionScanSpec* specOut) {
    (void)context;
    if (specOut == nullptr) {
        return false;
    }
    auto& reg = registry();
    lbug::function::TableFunction* scanFunction = nullptr;
    // Shared ownership: the lambdas below must stay copyable for std::function.
    std::shared_ptr<ParentSchema> schema;
    {
        std::unique_lock lock(reg.mutex);
        const auto schemaIt = reg.schemas.find(ref.parentTableID);
        if (schemaIt == reg.schemas.end()) {
            return false;
        }
        schema = std::make_shared<ParentSchema>(schemaIt->second);
        auto funcIt = reg.scanFunctions.find(ref.parentTableID);
        if (funcIt == reg.scanFunctions.end()) {
            auto func = std::make_unique<lbug::function::TableFunction>(
                "lbug_routing_scan_" + std::to_string(ref.parentTableID),
                std::vector<LogicalTypeID>{});
            // Direct calls (not the partitioned-scan path) get the full schema.
            // Built directly: Binder (and hence createVariables) is not amalgamated.
            func->bindFunc = [schema](lbug::main::ClientContext*,
                                   const TableFuncBindInput*) {
                return std::make_unique<TableFuncBindData>(scanColumns(*schema, "r"), 0);
            };
            const uint64_t parent = ref.parentTableID;
            func->tableFunc = lbug::function::SimpleTableFunc::getTableFunc(
                [parent](const TableFuncMorsel& morsel, const TableFuncInput&,
                    lbug::common::DataChunk& output) {
                    return scanParent(parent, morsel, output);
                });
            func->initSharedStateFunc = lbug::function::SimpleTableFunc::initSharedState;
            func->initLocalStateFunc = lbug::function::TableFunction::initEmptyLocalState;
            scanFunction = func.get();
            reg.scanFunctions.emplace(ref.parentTableID, std::move(func));
        } else {
            scanFunction = funcIt->second.get();
        }
    }
    specOut->scanFunction = scanFunction;
    const uint64_t parent = ref.parentTableID;
    specOut->createBindData = [schema, parent](const std::string& nodeUniqueName) {
        return std::make_unique<TableFuncBindData>(
            scanColumns(*schema, nodeUniqueName), storeSize(parent));
    };
    return true;
}

} // namespace

// --- C ABI ------------------------------------------------------------------

extern "C" {

int lbug_partition_routing_install(const lbug_partition_hooks_t* hooks) {
    if (hooks == nullptr) {
        return 1;
    }
    if (lbug::common::getPartitionRoutingHooks() != nullptr) {
        return 1;
    }
    auto& reg = registry();
    {
        std::unique_lock lock(reg.mutex);
        reg.callbacks = *hooks;
    }
    static lbug::common::PartitionRoutingHooks engineHooks{};
    engineHooks.context = hooks->context;
    engineHooks.locate = locateForward;
    engineHooks.onPartitionCreate = createForward;
    engineHooks.onPartitionDrop = dropForward;
    engineHooks.bindScan = bindScanForward;
    engineHooks.insertRow = insertRowForward;
    engineHooks.insertChunk = insertChunkForward;
    engineHooks.lookupRow = nullptr;
    lbug::common::setPartitionRoutingHooks(&engineHooks);
    return 0;
}

void lbug_partition_routing_uninstall(void) {
    lbug::common::setPartitionRoutingHooks(nullptr);
}

uint8_t lbug_partition_routing_is_installed(void) {
    return lbug::common::getPartitionRoutingHooks() != nullptr ? 1 : 0;
}

int lbug_partition_routing_register_schema(uint64_t parent_table_id,
    const char* const* prop_names, const uint8_t* type_ids, size_t n_props) {
    if ((n_props > 0 && (prop_names == nullptr || type_ids == nullptr)) || n_props == 0) {
        return 1;
    }
    ParentSchema schema;
    try {
        for (size_t i = 0; i < n_props; ++i) {
            if (prop_names[i] == nullptr) {
                return 1;
            }
            schema.propNames.emplace_back(prop_names[i]);
            schema.propTypeIDs.push_back(static_cast<LogicalTypeID>(type_ids[i]));
        }
    } catch (...) {
        return 1;
    }
    auto& reg = registry();
    std::unique_lock lock(reg.mutex);
    reg.schemas[parent_table_id] = std::move(schema);
    return 0;
}

} // extern "C"
