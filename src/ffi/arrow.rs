#[repr(transparent)]
pub struct ArrowArray(pub arrow::ffi::FFI_ArrowArray);

#[repr(transparent)]
pub struct ArrowSchema(pub arrow::ffi::FFI_ArrowSchema);

unsafe impl cxx::ExternType for ArrowArray {
    type Id = cxx::type_id!("ArrowArray");
    type Kind = cxx::kind::Trivial;
}

unsafe impl cxx::ExternType for ArrowSchema {
    type Id = cxx::type_id!("ArrowSchema");
    type Kind = cxx::kind::Trivial;
}

#[cxx::bridge]
pub(crate) mod ffi_arrow {
    unsafe extern "C++" {
        include!("lbug/include/lbug_arrow.h");
        include!("lbug/include/lbug_rs.h");

        #[namespace = "std"]
        #[cxx_name = "string_view"]
        type StringView<'a> = crate::ffi::StringView<'a>;

        #[namespace = "lbug::main"]
        type Connection<'db> = crate::ffi::ffi::Connection<'db>;

        #[namespace = "lbug::main"]
        type QueryResult<'db> = crate::ffi::ffi::QueryResult<'db>;
    }

    unsafe extern "C++" {
        type ArrowArray = crate::ffi::arrow::ArrowArray;

        #[namespace = "lbug_arrow"]
        fn query_result_has_next_arrow_chunk<'db>(result: Pin<&mut QueryResult<'db>>) -> bool;

        #[namespace = "lbug_arrow"]
        fn query_result_get_next_arrow_chunk<'db>(
            result: Pin<&mut QueryResult<'db>>,
            chunk_size: u64,
        ) -> Result<ArrowArray>;

        #[namespace = "lbug_arrow"]
        fn query_result_get_csr_indptr<'db>(result: &QueryResult<'db>) -> Result<ArrowArray>;

        #[namespace = "lbug_arrow"]
        fn query_result_get_csr_indices<'db>(result: &QueryResult<'db>) -> Result<ArrowArray>;

        #[namespace = "lbug_arrow"]
        fn query_result_get_csr_edge_ids<'db>(result: &QueryResult<'db>) -> Result<ArrowArray>;

        #[namespace = "lbug_arrow"]
        fn query_result_has_csr_edge_ids<'db>(result: &QueryResult<'db>) -> Result<bool>;
    }

    unsafe extern "C++" {
        type ArrowSchema = crate::ffi::arrow::ArrowSchema;

        #[namespace = "lbug_arrow"]
        fn query_result_get_arrow_schema<'db>(result: &QueryResult<'db>) -> Result<ArrowSchema>;
    }

    #[namespace = "lbug_rs"]
    unsafe extern "C++" {
        type ArrowArrayList;

        fn new_arrow_array_list() -> UniquePtr<ArrowArrayList>;

        fn arrow_array_list_push(list: Pin<&mut ArrowArrayList>, array: ArrowArray);

        fn connection_query_as_arrow<'a, 'db>(
            connection: Pin<&mut Connection<'db>>,
            query: StringView<'a>,
            chunk_size: i64,
        ) -> Result<UniquePtr<QueryResult<'db>>>;

        fn connection_create_arrow_table<'a, 'db>(
            connection: Pin<&mut Connection<'db>>,
            table_name: StringView<'a>,
            schema: ArrowSchema,
            arrays: UniquePtr<ArrowArrayList>,
        ) -> Result<UniquePtr<QueryResult<'db>>>;

        fn connection_create_arrow_rel_table<'a, 'b, 'c, 'db>(
            connection: Pin<&mut Connection<'db>>,
            table_name: StringView<'a>,
            src_table_name: StringView<'b>,
            dst_table_name: StringView<'c>,
            schema: ArrowSchema,
            arrays: UniquePtr<ArrowArrayList>,
        ) -> Result<UniquePtr<QueryResult<'db>>>;

        fn connection_create_arrow_rel_table_csr<'a, 'b, 'c, 'd, 'db>(
            connection: Pin<&mut Connection<'db>>,
            table_name: StringView<'a>,
            src_table_name: StringView<'b>,
            dst_table_name: StringView<'c>,
            indices_schema: ArrowSchema,
            indices_arrays: UniquePtr<ArrowArrayList>,
            indptr_schema: ArrowSchema,
            indptr_arrays: UniquePtr<ArrowArrayList>,
            dst_col_name: StringView<'d>,
        ) -> Result<UniquePtr<QueryResult<'db>>>;

        fn connection_drop_arrow_table<'a, 'db>(
            connection: Pin<&mut Connection<'db>>,
            table_name: StringView<'a>,
        ) -> Result<UniquePtr<QueryResult<'db>>>;
    }
}
