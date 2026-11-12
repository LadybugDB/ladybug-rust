use lbug::{Connection, ConversionError, Database, Error, SystemConfig};

#[test]
fn test_downstream_row_question_mark_and_error_match() -> Result<(), Error> {
    let temp_dir = tempfile::tempdir().unwrap();
    let db = Database::new(temp_dir.path().join("testdb"), SystemConfig::default())?;
    let conn = Connection::new(&db)?;
    let mut query_res = conn.query("RETURN 1;")?;
    for row in &mut query_res {
        let _row: Vec<lbug::Value> = row?; // Requires From<ConversionError> for Error
    }
    Ok(())
}

#[test]
fn test_error_source_chain_traversal() {
    let dec_err = rust_decimal::Error::ExceedsMaximumPossibleValue;
    let conv_err = ConversionError::Decimal(100, 2, dec_err.clone());
    let top_err: Error = conv_err.into();

    let source1 = std::error::Error::source(&top_err).expect("top_err should have source");
    let conv_ref = source1
        .downcast_ref::<ConversionError>()
        .expect("source1 should be ConversionError");
    match conv_ref {
        ConversionError::Decimal(val, scale, _) => {
            assert_eq!(*val, 100);
            assert_eq!(*scale, 2);
        }
        _ => panic!("expected ConversionError::Decimal"),
    }

    let source2 = std::error::Error::source(source1).expect("conv_err should have source");
    let dec_ref = source2
        .downcast_ref::<rust_decimal::Error>()
        .expect("source2 should be rust_decimal::Error");
    assert_eq!(*dec_ref, dec_err);
}
