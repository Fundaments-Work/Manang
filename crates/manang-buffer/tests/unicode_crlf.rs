use manang_buffer::{BufferError, DocumentBuffer, Position};

#[test]
fn test_unicode_multi_byte_character_integrity() {
    let sample = "🦀 Rust 🚀 CeTZ ∫_0^\\infty e^{-x} dx = 1 🔬 中文 नमस्ते";
    let mut buf = DocumentBuffer::open(sample);

    assert_eq!(buf.to_string(), sample);
    assert_eq!(buf.len_chars(), sample.chars().count());
    assert_eq!(buf.len_bytes(), sample.len());

    // Round-trip every character index to byte offset and back
    for (char_idx, (byte_offset, ch)) in sample.char_indices().enumerate() {
        let mapped_byte = buf.char_to_byte_idx(char_idx).unwrap();
        assert_eq!(mapped_byte, byte_offset, "char_idx {char_idx} '{ch}'");

        let mapped_char = buf.byte_to_char_idx(byte_offset).unwrap();
        assert_eq!(mapped_char, char_idx, "byte_offset {byte_offset} '{ch}'");
    }

    // Attempting byte offset inside a multi-byte sequence must return InvalidUtf8Boundary
    // '🦀' is 4 bytes (0, 1, 2, 3), so offsets 1, 2, 3 are inside the code point.
    assert!(matches!(
        buf.byte_to_char_idx(1),
        Err(BufferError::InvalidUtf8Boundary { byte_offset: 1 })
    ));
    assert!(matches!(
        buf.byte_to_char_idx(2),
        Err(BufferError::InvalidUtf8Boundary { byte_offset: 2 })
    ));
    assert!(matches!(
        buf.byte_to_char_idx(3),
        Err(BufferError::InvalidUtf8Boundary { byte_offset: 3 })
    ));
    // Offset 4 is the space character following the crab
    assert_eq!(buf.byte_to_char_idx(4).unwrap(), 1);

    // Insertion in the middle of Unicode
    buf.insert(1, " [CRAB] ").unwrap();
    assert!(buf.to_string().starts_with("🦀 [CRAB]  Rust"));

    // Undo insertion
    assert!(buf.undo().unwrap());
    assert_eq!(buf.to_string(), sample);
}

#[test]
fn test_crlf_and_mixed_line_endings() {
    let text = "Line 1\r\nLine 2\nLine 3\rLine 4\r\n";
    let buf = DocumentBuffer::open(text);

    // Verify line count
    assert_eq!(buf.line_count(), 5); // 4 content lines + 1 trailing empty line after \r\n

    // Line 0 is "Line 1\r\n" (8 chars)
    assert_eq!(buf.char_to_line_col(0).unwrap(), (0, 0));
    assert_eq!(buf.char_to_line_col(5).unwrap(), (0, 5)); // '1'
    assert_eq!(buf.char_to_line_col(6).unwrap(), (0, 6)); // '\r'
    assert_eq!(buf.char_to_line_col(7).unwrap(), (0, 7)); // '\n'
    assert_eq!(buf.char_to_line_col(8).unwrap(), (1, 0)); // 'L' in "Line 2"

    // Reverse conversions
    assert_eq!(buf.line_col_to_char(0, 0).unwrap(), 0);
    assert_eq!(buf.line_col_to_char(0, 7).unwrap(), 7);
    assert_eq!(buf.line_col_to_char(1, 0).unwrap(), 8);

    // Out of bounds line
    assert!(matches!(
        buf.line_col_to_char(10, 0),
        Err(BufferError::LineOutOfBounds { line: 10, .. })
    ));

    // Out of bounds column for line 0 (max col is 8)
    assert!(matches!(
        buf.line_col_to_char(0, 15),
        Err(BufferError::ColumnOutOfBounds {
            line: 0,
            col: 15,
            ..
        })
    ));
}

#[test]
fn test_nested_transactions_and_rollback() {
    let mut buf = DocumentBuffer::open("Initial state");

    buf.begin_transaction();
    buf.insert(0, "A: ").unwrap();

    // Nested transaction
    buf.begin_transaction();
    buf.insert(buf.len_chars(), " -> End").unwrap();
    buf.commit_transaction().unwrap();

    assert_eq!(buf.to_string(), "A: Initial state -> End");

    // Commit outer transaction
    buf.commit_transaction().unwrap();

    // Undo should undo both edits as one unit because they were enclosed in the outer transaction
    assert!(buf.undo().unwrap());
    assert_eq!(buf.to_string(), "Initial state");

    // Redo restores both
    assert!(buf.redo().unwrap());
    assert_eq!(buf.to_string(), "A: Initial state -> End");

    // Rollback test
    buf.begin_transaction();
    buf.insert(0, "Should be reverted: ").unwrap();
    assert!(buf.to_string().starts_with("Should be reverted"));
    buf.rollback_transaction().unwrap();
    assert_eq!(buf.to_string(), "A: Initial state -> End");
}

#[test]
fn test_transaction_closure_helper() {
    let mut buf = DocumentBuffer::open("hello");

    // Successful transaction
    let result = buf.transaction(|b| {
        b.insert(5, " world")?;
        b.insert(0, "Say: ")?;
        Ok(42)
    });
    assert_eq!(result.unwrap(), 42);
    assert_eq!(buf.to_string(), "Say: hello world");

    // Failed transaction rolls back automatically
    let err_result: Result<(), BufferError> = buf.transaction(|b| {
        b.insert(0, "Prefix ")?;
        // Intentional out of bounds error
        b.insert(9999, "Boom")?;
        Ok(())
    });
    assert!(err_result.is_err());
    assert_eq!(buf.to_string(), "Say: hello world");

    // Undo the successful transaction
    assert!(buf.undo().unwrap());
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn test_out_of_bounds_never_panics() {
    let mut buf = DocumentBuffer::open("test");

    assert!(buf.insert(100, "abc").is_err());
    assert!(buf.delete(2..10).is_err());
    assert!(buf.replace(10..20, "xyz").is_err());
    assert!(buf.slice(0..100).is_err());
    assert!(buf.char_to_position(100).is_err());
    assert!(buf.position_to_char(Position::new(10, 0)).is_err());
    assert!(buf.char_to_byte_idx(100).is_err());
    assert!(buf.byte_to_char_idx(100).is_err());
    assert!(buf.commit_transaction().is_err());
    assert!(buf.rollback_transaction().is_err());
}
