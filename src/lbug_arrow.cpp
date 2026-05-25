#include "lbug_arrow.h"

#include <optional>
#include <stdexcept>
#include <vector>

namespace lbug {
namespace main {

class ArrowQueryResult : public QueryResult {
public:
    struct CSRMetadata {
        std::vector<int64_t> indptr;
        std::vector<int64_t> indices;
        std::vector<int64_t> edgeIDs;
        bool hasEdgeIDs = false;
    };

    struct CSRArrowArray {
        ArrowArray array{};
        ArrowSchema schema{};

        CSRArrowArray() = default;
        ~CSRArrowArray() { release(); }
        CSRArrowArray(CSRArrowArray&& other) noexcept : array{other.array}, schema{other.schema} {
            other.array.release = nullptr;
            other.schema.release = nullptr;
        }
        CSRArrowArray& operator=(CSRArrowArray&& other) noexcept {
            if (this != &other) {
                release();
                array = other.array;
                schema = other.schema;
                other.array.release = nullptr;
                other.schema.release = nullptr;
            }
            return *this;
        }
        CSRArrowArray(const CSRArrowArray&) = delete;
        CSRArrowArray& operator=(const CSRArrowArray&) = delete;

        void release() {
            if (schema.release) {
                schema.release(&schema);
            }
            if (array.release) {
                array.release(&array);
            }
        }
    };

    struct CSRArrowArrays {
        CSRArrowArray indptr;
        CSRArrowArray indices;
        std::optional<CSRArrowArray> edgeIDs;
    };

    CSRArrowArrays getCSRArrowArrays() const;
};

} // namespace main
} // namespace lbug

namespace lbug_arrow {

ArrowSchema query_result_get_arrow_schema(const lbug::main::QueryResult& result) {
    // Could use directly, except that we can't (yet) mark ArrowSchema as being safe to store in a
    // cxx::UniquePtr
    return *result.getArrowSchema();
}

bool query_result_has_next_arrow_chunk(lbug::main::QueryResult& result) {
    return result.hasNextArrowChunk();
}

ArrowArray query_result_get_next_arrow_chunk(lbug::main::QueryResult& result, uint64_t chunkSize) {
    return *result.getNextArrowChunk(chunkSize);
}

static const lbug::main::ArrowQueryResult& get_arrow_query_result(
    const lbug::main::QueryResult& result) {
    auto arrowResult = dynamic_cast<const lbug::main::ArrowQueryResult*>(&result);
    if (arrowResult == nullptr) {
        throw std::runtime_error(
            "CSR export is only supported for Arrow query results with native CSR metadata.");
    }
    return *arrowResult;
}

static ArrowArray detach(lbug::main::ArrowQueryResult::CSRArrowArray& array) {
    auto result = array.array;
    array.array.release = nullptr;
    return result;
}

ArrowArray query_result_get_csr_indptr(const lbug::main::QueryResult& result) {
    auto arrays = get_arrow_query_result(result).getCSRArrowArrays();
    return detach(arrays.indptr);
}

ArrowArray query_result_get_csr_indices(const lbug::main::QueryResult& result) {
    auto arrays = get_arrow_query_result(result).getCSRArrowArrays();
    return detach(arrays.indices);
}

ArrowArray query_result_get_csr_edge_ids(const lbug::main::QueryResult& result) {
    auto arrays = get_arrow_query_result(result).getCSRArrowArrays();
    if (!arrays.edgeIDs.has_value()) {
        throw std::runtime_error("Arrow query result does not have CSR edge ids.");
    }
    return detach(*arrays.edgeIDs);
}

bool query_result_has_csr_edge_ids(const lbug::main::QueryResult& result) {
    return get_arrow_query_result(result).getCSRArrowArrays().edgeIDs.has_value();
}

} // namespace lbug_arrow
