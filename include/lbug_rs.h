#pragma once

#include <cstdint>
#include <array>
#include <memory>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

#include "rust/cxx.h"
#ifdef LBUG_BUNDLED
#include "c_api/lbug.h"
#include "main/lbug.h"
#else
#include <lbug.hpp>
#include <lbug.h>
#endif

namespace lbug_rs {

struct TypeListBuilder {
    std::vector<lbug::common::LogicalType> types;

    void insert(std::unique_ptr<lbug::common::LogicalType> type) {
        types.push_back(std::move(*type));
    }
};

std::unique_ptr<TypeListBuilder> create_type_list();
inline void type_list_insert(TypeListBuilder& list, std::unique_ptr<lbug::common::LogicalType> type) {
    list.insert(std::move(type));
}

struct QueryParams {
    std::unordered_map<std::string, std::unique_ptr<lbug::common::Value>> inputParams;

    void insert(const rust::Str key, std::unique_ptr<lbug::common::Value> value) {
        inputParams.insert(std::make_pair(key, std::move(value)));
    }
};

std::unique_ptr<QueryParams> new_params();
inline void query_params_insert(QueryParams& params, const rust::Str key,
    std::unique_ptr<lbug::common::Value> value) {
    params.insert(key, std::move(value));
}

std::unique_ptr<lbug::common::LogicalType> create_logical_type(lbug::common::LogicalTypeID id);
std::unique_ptr<lbug::common::LogicalType> create_logical_type_list(
    std::unique_ptr<lbug::common::LogicalType> childType);
std::unique_ptr<lbug::common::LogicalType> create_logical_type_array(
    std::unique_ptr<lbug::common::LogicalType> childType, uint64_t numElements);

inline std::unique_ptr<lbug::common::LogicalType> create_logical_type_struct(
    const rust::Vec<rust::String>& fieldNames, std::unique_ptr<TypeListBuilder> fieldTypes) {
    std::vector<lbug::common::StructField> fields;
    for (auto i = 0u; i < fieldNames.size(); i++) {
        fields.emplace_back(std::string(fieldNames[i]), std::move(fieldTypes->types[i]));
    }
    return std::make_unique<lbug::common::LogicalType>(
        lbug::common::LogicalType::STRUCT(std::move(fields)));
}
inline std::unique_ptr<lbug::common::LogicalType> create_logical_type_union(
    const rust::Vec<rust::String>& fieldNames, std::unique_ptr<TypeListBuilder> fieldTypes) {
    std::vector<lbug::common::StructField> fields;
    for (auto i = 0u; i < fieldNames.size(); i++) {
        fields.emplace_back(std::string(fieldNames[i]), std::move(fieldTypes->types[i]));
    }
    return std::make_unique<lbug::common::LogicalType>(
        lbug::common::LogicalType::UNION(std::move(fields)));
}
std::unique_ptr<lbug::common::LogicalType> create_logical_type_map(
    std::unique_ptr<lbug::common::LogicalType> keyType,
    std::unique_ptr<lbug::common::LogicalType> valueType);

inline std::unique_ptr<lbug::common::LogicalType> create_logical_type_decimal(uint32_t precision,
    uint32_t scale) {
    return std::make_unique<lbug::common::LogicalType>(
        lbug::common::LogicalType::DECIMAL(precision, scale));
}

std::unique_ptr<lbug::common::LogicalType> logical_type_get_list_child_type(
    const lbug::common::LogicalType& logicalType);
std::unique_ptr<lbug::common::LogicalType> logical_type_get_array_child_type(
    const lbug::common::LogicalType& logicalType);
uint64_t logical_type_get_array_num_elements(const lbug::common::LogicalType& logicalType);

rust::Vec<rust::String> logical_type_get_struct_field_names(const lbug::common::LogicalType& value);
std::unique_ptr<std::vector<lbug::common::LogicalType>> logical_type_get_struct_field_types(
    const lbug::common::LogicalType& value);

inline uint32_t logical_type_get_decimal_precision(const lbug::common::LogicalType& logicalType) {
    return lbug::common::DecimalType::getPrecision(logicalType);
}
inline uint32_t logical_type_get_decimal_scale(const lbug::common::LogicalType& logicalType) {
    return lbug::common::DecimalType::getScale(logicalType);
}
inline lbug::common::LogicalTypeID logical_type_get_logical_type_id(
    const lbug::common::LogicalType& logicalType) {
    return logicalType.getLogicalTypeID();
}

/* Database */
std::unique_ptr<lbug::main::Database> new_database(std::string_view databasePath,
    uint64_t bufferPoolSize, uint64_t maxNumThreads, bool enableCompression, bool readOnly,
    uint64_t maxDBSize, bool autoCheckpoint, int64_t checkpointThreshold,
    bool throwOnWalReplayFailure, bool enableChecksums, bool enableMultiWrites);

void database_set_logging_level(lbug::main::Database& database, const std::string& level);

/* Connection */
std::unique_ptr<lbug::main::Connection> database_connect(lbug::main::Database& database);
std::unique_ptr<lbug::main::QueryResult> connection_execute(lbug::main::Connection& connection,
    lbug::main::PreparedStatement& query, std::unique_ptr<QueryParams> params);
inline std::unique_ptr<lbug::main::QueryResult> connection_query(lbug::main::Connection& connection,
    std::string_view query) {
    return connection.query(query);
}
inline std::unique_ptr<lbug::main::PreparedStatement> connection_prepare(
    lbug::main::Connection& connection, std::string_view query) {
    return connection.prepare(query);
}
inline uint64_t connection_get_max_num_thread_for_exec(lbug::main::Connection& connection) {
    return connection.getMaxNumThreadForExec();
}
inline void connection_set_max_num_thread_for_exec(lbug::main::Connection& connection,
    uint64_t numThreads) {
    connection.setMaxNumThreadForExec(numThreads);
}
inline void connection_interrupt(lbug::main::Connection& connection) {
    connection.interrupt();
}
inline void connection_set_query_timeout(lbug::main::Connection& connection, uint64_t timeoutMs) {
    connection.setQueryTimeOut(timeoutMs);
}

/* PreparedStatement */
rust::String prepared_statement_error_message(const lbug::main::PreparedStatement& statement);
inline bool prepared_statement_is_read_only(const lbug::main::PreparedStatement& statement) {
    lbug_prepared_statement c_statement{
        const_cast<lbug::main::PreparedStatement*>(&statement), nullptr};
    return lbug_prepared_statement_is_read_only(&c_statement);
}
inline bool prepared_statement_is_success(const lbug::main::PreparedStatement& statement) {
    return statement.isSuccess();
}

/* QueryResult */
rust::String query_result_to_string(const lbug::main::QueryResult& result);
rust::String query_result_get_error_message(const lbug::main::QueryResult& result);
inline bool query_result_is_success(const lbug::main::QueryResult& result) {
    return result.isSuccess();
}
inline bool query_result_has_next(const lbug::main::QueryResult& result) {
    return result.hasNext();
}
inline std::shared_ptr<lbug::processor::FlatTuple> query_result_get_next(
    lbug::main::QueryResult& result) {
    return result.getNext();
}

double query_result_get_compiling_time(const lbug::main::QueryResult& result);
double query_result_get_execution_time(const lbug::main::QueryResult& result);
inline size_t query_result_get_num_columns(const lbug::main::QueryResult& result) {
    return result.getNumColumns();
}
inline uint64_t query_result_get_num_tuples(const lbug::main::QueryResult& result) {
    return result.getNumTuples();
}

std::unique_ptr<std::vector<lbug::common::LogicalType>> query_result_column_data_types(
    const lbug::main::QueryResult& query_result);
rust::Vec<rust::String> query_result_column_names(const lbug::main::QueryResult& query_result);

/* NodeVal/RelVal */
rust::String node_value_get_label_name(const lbug::common::Value& val);
rust::String rel_value_get_label_name(const lbug::common::Value& val);

size_t node_value_get_num_properties(const lbug::common::Value& value);
size_t rel_value_get_num_properties(const lbug::common::Value& value);

rust::String node_value_get_property_name(const lbug::common::Value& value, size_t index);
rust::String rel_value_get_property_name(const lbug::common::Value& value, size_t index);

const lbug::common::Value& node_value_get_property_value(const lbug::common::Value& value,
    size_t index);
const lbug::common::Value& rel_value_get_property_value(const lbug::common::Value& value,
    size_t index);

/* NodeVal */
const lbug::common::Value& node_value_get_node_id(const lbug::common::Value& val);

/* RelVal */
const lbug::common::Value& rel_value_get_src_id(const lbug::common::Value& val);
std::array<uint64_t, 2> rel_value_get_dst_id(const lbug::common::Value& val);

/* RecursiveRel */
const lbug::common::Value& recursive_rel_get_nodes(const lbug::common::Value& val);
const lbug::common::Value& recursive_rel_get_rels(const lbug::common::Value& val);

/* FlatTuple */
uint32_t flat_tuple_len(const lbug::processor::FlatTuple& flatTuple);
const lbug::common::Value& flat_tuple_get_value(const lbug::processor::FlatTuple& flatTuple,
    uint32_t index);

/* Value */
const std::string& value_get_string(const lbug::common::Value& value);

template<typename T>
std::unique_ptr<T> value_get_unique(const lbug::common::Value& value) {
    return std::make_unique<T>(value.getValue<T>());
}

int64_t value_get_interval_secs(const lbug::common::Value& value);
int32_t value_get_interval_micros(const lbug::common::Value& value);
int32_t value_get_date_days(const lbug::common::Value& value);
int64_t value_get_timestamp_ns(const lbug::common::Value& value);
int64_t value_get_timestamp_ms(const lbug::common::Value& value);
int64_t value_get_timestamp_sec(const lbug::common::Value& value);
int64_t value_get_timestamp_micros(const lbug::common::Value& value);
int64_t value_get_timestamp_tz(const lbug::common::Value& value);
std::array<uint64_t, 2> value_get_int128_t(const lbug::common::Value& value);
std::array<uint64_t, 2> value_get_internal_id(const lbug::common::Value& value);
uint32_t value_get_children_size(const lbug::common::Value& value);
const lbug::common::Value& value_get_child(const lbug::common::Value& value, uint32_t index);
lbug::common::LogicalTypeID value_get_data_type_id(const lbug::common::Value& value);
const lbug::common::LogicalType& value_get_data_type(const lbug::common::Value& value);
inline lbug::common::PhysicalTypeID value_get_physical_type(const lbug::common::Value& value) {
    return value.getDataType().getPhysicalType();
}
rust::String value_to_string(const lbug::common::Value& val);
inline bool value_get_bool(const lbug::common::Value& value) {
    return value.getValue<bool>();
}
inline int8_t value_get_i8(const lbug::common::Value& value) {
    return value.getValue<int8_t>();
}
inline int16_t value_get_i16(const lbug::common::Value& value) {
    return value.getValue<int16_t>();
}
inline int32_t value_get_i32(const lbug::common::Value& value) {
    return value.getValue<int32_t>();
}
inline int64_t value_get_i64(const lbug::common::Value& value) {
    return value.getValue<int64_t>();
}
inline uint8_t value_get_u8(const lbug::common::Value& value) {
    return value.getValue<uint8_t>();
}
inline uint16_t value_get_u16(const lbug::common::Value& value) {
    return value.getValue<uint16_t>();
}
inline uint32_t value_get_u32(const lbug::common::Value& value) {
    return value.getValue<uint32_t>();
}
inline uint64_t value_get_u64(const lbug::common::Value& value) {
    return value.getValue<uint64_t>();
}
inline float value_get_float(const lbug::common::Value& value) {
    return value.getValue<float>();
}
inline double value_get_double(const lbug::common::Value& value) {
    return value.getValue<double>();
}
inline bool value_is_null(const lbug::common::Value& value) {
    return value.isNull();
}

std::unique_ptr<lbug::common::Value> create_value_string(lbug::common::LogicalTypeID typ,
    const rust::Slice<const unsigned char> value);
std::unique_ptr<lbug::common::Value> create_value_timestamp(const int64_t timestamp);
std::unique_ptr<lbug::common::Value> create_value_timestamp_tz(const int64_t timestamp);
std::unique_ptr<lbug::common::Value> create_value_timestamp_ns(const int64_t timestamp);
std::unique_ptr<lbug::common::Value> create_value_timestamp_ms(const int64_t timestamp);
std::unique_ptr<lbug::common::Value> create_value_timestamp_sec(const int64_t timestamp);
inline std::unique_ptr<lbug::common::Value> create_value_date(const int32_t date) {
    return std::make_unique<lbug::common::Value>(lbug::common::date_t(date));
}
std::unique_ptr<lbug::common::Value> create_value_interval(const int32_t months, const int32_t days,
    const int64_t micros);
std::unique_ptr<lbug::common::Value> create_value_null(
    std::unique_ptr<lbug::common::LogicalType> typ);
std::unique_ptr<lbug::common::Value> create_value_int128_t(int64_t high, uint64_t low);
std::unique_ptr<lbug::common::Value> create_value_internal_id(uint64_t offset, uint64_t table);

inline std::unique_ptr<lbug::common::Value> create_value_uuid_t(int64_t high, uint64_t low) {
    return std::make_unique<lbug::common::Value>(
        lbug::common::uuid{lbug::common::int128_t(low, high)});
}

template<typename T>
std::unique_ptr<lbug::common::Value> create_value(const T value) {
    return std::make_unique<lbug::common::Value>(value);
}
inline std::unique_ptr<lbug::common::Value> create_value_decimal(int64_t high, uint64_t low,
    uint32_t scale, uint32_t precision) {
    auto value =
        std::make_unique<lbug::common::Value>(lbug::common::LogicalType::DECIMAL(precision, scale),
            std::vector<std::unique_ptr<lbug::common::Value>>{});
    auto i128 = lbug::common::int128_t(low, high);
    lbug::common::TypeUtils::visit(
        value->getDataType().getPhysicalType(),
        [&](lbug::common::int128_t) { value->val.int128Val = i128; },
        [&](int64_t) { value->val.int64Val = static_cast<int64_t>(i128); },
        [&](int32_t) { value->val.int32Val = static_cast<int32_t>(i128); },
        [&](int16_t) { value->val.int16Val = static_cast<int16_t>(i128); },
        [](auto) { UNREACHABLE_CODE; });
    return value;
}

struct ValueListBuilder {
    std::vector<std::unique_ptr<lbug::common::Value>> values;

    void insert(std::unique_ptr<lbug::common::Value> value) { values.push_back(std::move(value)); }
};

std::unique_ptr<lbug::common::Value> get_list_value(std::unique_ptr<lbug::common::LogicalType> typ,
    std::unique_ptr<ValueListBuilder> value);
std::unique_ptr<ValueListBuilder> create_list();
inline void value_list_insert(ValueListBuilder& list, std::unique_ptr<lbug::common::Value> value) {
    list.insert(std::move(value));
}

inline std::string_view string_view_from_str(rust::Str s) {
    return {s.data(), s.size()};
}

inline lbug::storage::storage_version_t get_storage_version() {
    return lbug::storage::StorageVersionInfo::getStorageVersion();
}

} // namespace lbug_rs
