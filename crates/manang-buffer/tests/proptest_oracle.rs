use manang_buffer::DocumentBuffer;
use proptest::prelude::*;

#[derive(Debug, Clone, Default)]
struct StringOracle {
    text: String,
}

impl StringOracle {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
        }
    }

    fn len_chars(&self) -> usize {
        self.text.chars().count()
    }

    fn insert(&mut self, char_idx: usize, insert_str: &str) -> Result<(), ()> {
        let total = self.len_chars();
        if char_idx > total {
            return Err(());
        }
        let byte_pos = if char_idx == total {
            self.text.len()
        } else {
            self.text
                .char_indices()
                .nth(char_idx)
                .map(|(b, _)| b)
                .unwrap_or(self.text.len())
        };
        self.text.insert_str(byte_pos, insert_str);
        Ok(())
    }

    fn delete(&mut self, start: usize, end: usize) -> Result<String, ()> {
        let total = self.len_chars();
        if start > end || end > total {
            return Err(());
        }
        let byte_start = if start == total {
            self.text.len()
        } else {
            self.text
                .char_indices()
                .nth(start)
                .map(|(b, _)| b)
                .unwrap_or(self.text.len())
        };
        let byte_end = if end == total {
            self.text.len()
        } else {
            self.text
                .char_indices()
                .nth(end)
                .map(|(b, _)| b)
                .unwrap_or(self.text.len())
        };
        let deleted: String = self.text[byte_start..byte_end].to_owned();
        self.text.drain(byte_start..byte_end);
        Ok(deleted)
    }

    fn replace(&mut self, start: usize, end: usize, new_text: &str) -> Result<String, ()> {
        let deleted = self.delete(start, end)?;
        self.insert(start, new_text)?;
        Ok(deleted)
    }
}

#[derive(Debug, Clone)]
enum Op {
    Insert {
        char_idx: usize,
        text: String,
    },
    Delete {
        start: usize,
        end: usize,
    },
    Replace {
        start: usize,
        end: usize,
        text: String,
    },
    Slice {
        start: usize,
        end: usize,
    },
}

fn arbitrary_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        (0..1000usize, "[a-zA-Z0-9 \\n\\t🦀∑]{0,20}")
            .prop_map(|(char_idx, text)| Op::Insert { char_idx, text }),
        (0..1000usize, 0..1000usize).prop_map(|(start, end)| Op::Delete { start, end }),
        (0..1000usize, 0..1000usize, "[a-zA-Z0-9 \\n\\t🦀∑]{0,20}")
            .prop_map(|(start, end, text)| Op::Replace { start, end, text }),
        (0..1000usize, 0..1000usize).prop_map(|(start, end)| Op::Slice { start, end }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn test_random_ops_against_oracle(ops in proptest::collection::vec(arbitrary_op(), 1..100)) {
        let mut buffer = DocumentBuffer::new();
        let mut oracle = StringOracle::new("");

        for op in ops {
            match op {
                Op::Insert { char_idx, text } => {
                    let res_buf = buffer.insert(char_idx, &text);
                    let res_ora = oracle.insert(char_idx, &text);
                    prop_assert_eq!(res_buf.is_ok(), res_ora.is_ok());
                    let current = buffer.to_string();
                    prop_assert_eq!(&current, &oracle.text);
                }
                Op::Delete { start, end } => {
                    let res_buf = buffer.delete(start..end);
                    let res_ora = oracle.delete(start, end);
                    prop_assert_eq!(res_buf.is_ok(), res_ora.is_ok());
                    if let (Ok(del_buf), Ok(del_ora)) = (res_buf, res_ora) {
                        prop_assert_eq!(del_buf, del_ora);
                    }
                    let current = buffer.to_string();
                    prop_assert_eq!(&current, &oracle.text);
                }
                Op::Replace { start, end, text } => {
                    let res_buf = buffer.replace(start..end, &text);
                    let res_ora = oracle.replace(start, end, &text);
                    prop_assert_eq!(res_buf.is_ok(), res_ora.is_ok());
                    if let (Ok(del_buf), Ok(del_ora)) = (res_buf, res_ora) {
                        prop_assert_eq!(del_buf, del_ora);
                    }
                    let current = buffer.to_string();
                    prop_assert_eq!(&current, &oracle.text);
                }
                Op::Slice { start, end } => {
                    let res_buf = buffer.slice(start..end);
                    let total = oracle.len_chars();
                    let valid_range = start <= end && end <= total;
                    prop_assert_eq!(res_buf.is_ok(), valid_range);
                    if let Ok(sliced) = res_buf {
                        let expected = oracle.text.chars().skip(start).take(end - start).collect::<String>();
                        prop_assert_eq!(sliced, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn test_undo_redo_round_trip(
        initial in "[a-zA-Z0-9 \\n]{0,50}",
        edits in proptest::collection::vec(
            (0..50usize, "[a-z0-9]{1,5}"),
            1..20
        )
    ) {
        let mut buffer = DocumentBuffer::open(&initial);
        let baseline = buffer.to_string();

        let mut applied_count = 0;
        for (char_idx, text) in edits {
            let idx = char_idx % (buffer.len_chars() + 1);
            if buffer.insert(idx, &text).is_ok() {
                applied_count += 1;
            }
        }

        let modified = buffer.to_string();

        // Undo all
        for _ in 0..applied_count {
            prop_assert!(buffer.can_undo());
            prop_assert!(buffer.undo().unwrap());
        }
        prop_assert_eq!(buffer.to_string(), baseline);

        // Redo all
        for _ in 0..applied_count {
            prop_assert!(buffer.can_redo());
            prop_assert!(buffer.redo().unwrap());
        }
        prop_assert_eq!(buffer.to_string(), modified);
    }
}
