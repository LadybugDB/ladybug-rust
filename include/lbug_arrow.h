#pragma once

#include "rust/cxx.h"
#ifdef LBUG_BUNDLED
#include "main/lbug.h"
#else
#include <lbug.hpp>
#endif

namespace lbug_arrow {

ArrowSchema query_result_get_arrow_schema(const lbug::main::QueryResult& result);
bool query_result_has_next_arrow_chunk(lbug::main::QueryResult& result);
ArrowArray query_result_get_next_arrow_chunk(lbug::main::QueryResult& result, uint64_t chunkSize);
ArrowArray query_result_get_csr_indptr(const lbug::main::QueryResult& result);
ArrowArray query_result_get_csr_indices(const lbug::main::QueryResult& result);
ArrowArray query_result_get_csr_edge_ids(const lbug::main::QueryResult& result);
bool query_result_has_csr_edge_ids(const lbug::main::QueryResult& result);

} // namespace lbug_arrow
