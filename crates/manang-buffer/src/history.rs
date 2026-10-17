use serde::{Deserialize, Serialize};

use crate::error::BufferError;

/// A primitive text edit operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edit {
    /// Insertion of text at a given character index.
    Insert {
        /// The character index where text was inserted.
        char_idx: usize,
        /// The inserted text.
        text: String,
    },
    /// Deletion of text at a given character index.
    Delete {
        /// The character index where text was deleted.
        char_idx: usize,
        /// The deleted text.
        text: String,
    },
}

impl Edit {
    /// Produces the inverse edit that reverts this change.
    pub fn inverted(self) -> Self {
        match self {
            Edit::Insert { char_idx, text } => Edit::Delete { char_idx, text },
            Edit::Delete { char_idx, text } => Edit::Insert { char_idx, text },
        }
    }
}

/// An atomic sequence of edits grouped together as a transaction.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Transaction {
    /// The sequential edits comprising this transaction.
    pub edits: Vec<Edit>,
}

impl Transaction {
    /// Creates a new empty transaction.
    pub fn new() -> Self {
        Self { edits: Vec::new() }
    }

    /// Whether this transaction contains no edits.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// Records an edit into this transaction.
    pub fn push(&mut self, edit: Edit) {
        self.edits.push(edit);
    }
}

/// The undo/redo history manager.
#[derive(Debug, Clone)]
pub struct History {
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    active_transaction: Option<Transaction>,
    transaction_depth: usize,
    max_history: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl History {
    /// Creates a new history manager with a maximum undo stack capacity.
    pub fn new(max_history: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            active_transaction: None,
            transaction_depth: 0,
            max_history,
        }
    }

    /// Whether an explicit transaction is currently in progress.
    pub fn is_in_transaction(&self) -> bool {
        self.transaction_depth > 0
    }

    /// Begins a new transaction (supports nesting).
    pub fn begin_transaction(&mut self) {
        if self.transaction_depth == 0 {
            self.active_transaction = Some(Transaction::new());
        }
        self.transaction_depth += 1;
    }

    /// Commits an active transaction.
    /// Returns `Some(Transaction)` if the outermost transaction closed, or `None` if nested.
    /// Returns an error if no transaction was active.
    pub fn commit_transaction(&mut self) -> Result<Option<Transaction>, BufferError> {
        if self.transaction_depth == 0 {
            return Err(BufferError::NoActiveTransaction);
        }
        self.transaction_depth -= 1;
        if self.transaction_depth == 0 {
            let tx = self.active_transaction.take().unwrap_or_default();
            if !tx.is_empty() {
                self.push_undo(tx.clone());
                self.redo_stack.clear();
            }
            Ok(Some(tx))
        } else {
            Ok(None)
        }
    }

    /// Rolls back the active transaction and returns its recorded edits for reversal.
    pub fn rollback_transaction(&mut self) -> Result<Transaction, BufferError> {
        if self.transaction_depth == 0 {
            return Err(BufferError::NoActiveTransaction);
        }
        self.transaction_depth = 0;
        let tx = self.active_transaction.take().unwrap_or_default();
        Ok(tx)
    }

    /// Records an edit into the history.
    pub fn record_edit(&mut self, edit: Edit) {
        if let Some(ref mut tx) = self.active_transaction {
            tx.push(edit);
        } else {
            let mut tx = Transaction::new();
            tx.push(edit);
            self.push_undo(tx);
            self.redo_stack.clear();
        }
    }

    /// Pushes a transaction onto the undo stack, respecting `max_history`.
    fn push_undo(&mut self, tx: Transaction) {
        if self.undo_stack.len() >= self.max_history && !self.undo_stack.is_empty() {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(tx);
    }

    /// Whether an undo operation is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() && self.transaction_depth == 0
    }

    /// Whether a redo operation is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty() && self.transaction_depth == 0
    }

    /// Pops the latest transaction for undoing, placing it on the redo stack.
    pub fn pop_undo(&mut self) -> Option<Transaction> {
        if self.transaction_depth > 0 {
            return None;
        }
        let tx = self.undo_stack.pop()?;
        self.redo_stack.push(tx.clone());
        Some(tx)
    }

    /// Pops the latest transaction for redoing, placing it back on the undo stack.
    pub fn pop_redo(&mut self) -> Option<Transaction> {
        if self.transaction_depth > 0 {
            return None;
        }
        let tx = self.redo_stack.pop()?;
        self.push_undo(tx.clone());
        Some(tx)
    }

    /// Clears both undo and redo stacks and cancels any active transaction.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.active_transaction = None;
        self.transaction_depth = 0;
    }
}
