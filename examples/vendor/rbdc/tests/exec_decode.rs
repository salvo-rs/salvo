//! Tests for exec_decode JSON object format output
//!
//! exec_decode returns Value::Array where:
//! - Each element is a Value::Map representing a row
//! - Keys are column names, values are the row values
//! Format: [{"col1": "val1", "col2": "val2"}, {"col1": "val3", "col2": "val4"}]

use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use futures_util::StreamExt;
use rbdc::db::{Connection, MetaData, Row};
use rbdc::try_stream;
use rbs::Value;
use std::fmt::Debug;

// Mock MetaData for testing
#[derive(Debug, Clone)]
struct MockMetaData {
    column_names: Vec<String>,
}

impl MockMetaData {
    fn new(names: Vec<&str>) -> Self {
        Self {
            column_names: names.into_iter().map(|s| s.to_string()).collect(),
        }
    }
}

impl MetaData for MockMetaData {
    fn column_len(&self) -> usize {
        self.column_names.len()
    }

    fn column_name(&self, i: usize) -> String {
        self.column_names[i].clone()
    }

    fn column_type(&self, _i: usize) -> String {
        "TEXT".to_string()
    }
}

// Mock Row that stores values
#[derive(Debug)]
struct MockRow {
    metadata: MockMetaData,
    values: Vec<Value>,
}

impl MockRow {
    fn new(cols: Vec<&str>, vals: Vec<Value>) -> Self {
        Self {
            metadata: MockMetaData::new(cols),
            values: vals,
        }
    }
}

impl Row for MockRow {
    fn meta_data(&self) -> Box<dyn MetaData> {
        Box::new(MockMetaData {
            column_names: self.metadata.column_names.clone(),
        })
    }

    fn get(&mut self, i: usize) -> Result<Value, rbdc::Error> {
        if i >= self.values.len() {
            return Ok(Value::Null);
        }
        Ok(self.values[i].clone())
    }
}

// Mock Connection that returns predefined rows
#[derive(Debug)]
struct MockConnection {
    rows: std::sync::Mutex<Vec<Box<dyn Row>>>,
}

impl MockConnection {
    fn with_rows(rows: Vec<Box<dyn Row>>) -> Self {
        Self {
            rows: std::sync::Mutex::new(rows),
        }
    }
}

impl Connection for MockConnection {
    fn exec_rows(
        &mut self,
        _sql: &str,
        _params: Vec<Value>,
    ) -> BoxFuture<'_, Result<BoxStream<'_, Result<Box<dyn Row>, rbdc::Error>>, rbdc::Error>> {
        let rows: Vec<Box<dyn Row>> = self.rows.lock().unwrap().drain(..).collect();
        Box::pin(async move {
            let stream = try_stream! {
                for row in rows {
                    r#yield!(row);
                }
                Ok(())
            }
            .boxed();
            Ok(stream as BoxStream<Result<Box<dyn Row>, rbdc::Error>>)
        })
    }

    fn exec(
        &mut self,
        _sql: &str,
        _params: Vec<Value>,
    ) -> BoxFuture<'_, Result<rbdc::db::ExecResult, rbdc::Error>> {
        Box::pin(async move { Ok(rbdc::db::ExecResult::default()) })
    }

    fn ping(&mut self) -> BoxFuture<'_, Result<(), rbdc::Error>> {
        Box::pin(async { Ok(()) })
    }

    fn close(&mut self) -> BoxFuture<'_, Result<(), rbdc::Error>> {
        Box::pin(async { Ok(()) })
    }
}

fn exec_decode_sync(conn: &mut dyn Connection, sql: &str) -> Result<Value, rbdc::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(conn.exec_decode(sql, vec![]))
}

#[test]
fn test_exec_decode_empty_result() {
    let conn = MockConnection::with_rows(vec![]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT * FROM empty");
    let value = result.unwrap();
    match value {
        Value::Array(arr) => {
            assert!(arr.is_empty(), "Empty result should give empty array");
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_exec_decode_single_row_single_column() {
    let row = MockRow::new(vec!["id"], vec![Value::I64(42)]);
    let conn = MockConnection::with_rows(vec![Box::new(row)]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT id FROM t");
    let value = result.unwrap();
    match value {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 1);
            match &arr[0] {
                Value::Map(m) => {
                    assert_eq!(m.len(), 1);
                    let v = m.get(&Value::String("id".to_string()));
                    assert_eq!(*v, Value::I64(42));
                }
                _ => panic!("Expected Map"),
            }
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_exec_decode_multiple_columns_multiple_rows() {
    let row1 = MockRow::new(
        vec!["id", "name", "score"],
        vec![
            Value::I64(1),
            Value::String("Alice".to_string()),
            Value::I64(100),
        ],
    );
    let row2 = MockRow::new(
        vec!["id", "name", "score"],
        vec![
            Value::I64(2),
            Value::String("Bob".to_string()),
            Value::I64(85),
        ],
    );
    let conn = MockConnection::with_rows(vec![Box::new(row1), Box::new(row2)]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT id, name, score FROM t");
    let value = result.unwrap();
    match value {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 2);

            // Row 1
            match &arr[0] {
                Value::Map(m) => {
                    assert_eq!(*m.get(&Value::String("id".to_string())), Value::I64(1));
                    assert_eq!(
                        *m.get(&Value::String("name".to_string())),
                        Value::String("Alice".to_string())
                    );
                    assert_eq!(*m.get(&Value::String("score".to_string())), Value::I64(100));
                }
                _ => panic!("Expected Map"),
            }

            // Row 2
            match &arr[1] {
                Value::Map(m) => {
                    assert_eq!(*m.get(&Value::String("id".to_string())), Value::I64(2));
                    assert_eq!(
                        *m.get(&Value::String("name".to_string())),
                        Value::String("Bob".to_string())
                    );
                    assert_eq!(*m.get(&Value::String("score".to_string())), Value::I64(85));
                }
                _ => panic!("Expected Map"),
            }
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_exec_decode_with_null_values() {
    let row = MockRow::new(
        vec!["id", "name", "email"],
        vec![
            Value::I64(1),
            Value::String("Alice".to_string()),
            Value::Null,
        ],
    );
    let conn = MockConnection::with_rows(vec![Box::new(row)]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT id, name, email FROM t");
    let value = result.unwrap();
    match value {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 1);
            match &arr[0] {
                Value::Map(m) => {
                    assert_eq!(*m.get(&Value::String("id".to_string())), Value::I64(1));
                    assert_eq!(
                        *m.get(&Value::String("name".to_string())),
                        Value::String("Alice".to_string())
                    );
                    assert_eq!(*m.get(&Value::String("email".to_string())), Value::Null);
                }
                _ => panic!("Expected Map"),
            }
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_exec_decode_various_types() {
    let row = MockRow::new(
        vec!["int_val", "str_val", "float_val", "bool_val"],
        vec![
            Value::I64(42),
            Value::String("hello".to_string()),
            Value::F64(3.14),
            Value::Bool(true),
        ],
    );
    let conn = MockConnection::with_rows(vec![Box::new(row)]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT * FROM t");
    let value = result.unwrap();
    match value {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 1);
            match &arr[0] {
                Value::Map(m) => {
                    assert_eq!(
                        *m.get(&Value::String("int_val".to_string())),
                        Value::I64(42)
                    );
                    assert_eq!(
                        *m.get(&Value::String("str_val".to_string())),
                        Value::String("hello".to_string())
                    );
                    assert_eq!(
                        *m.get(&Value::String("float_val".to_string())),
                        Value::F64(3.14)
                    );
                    assert_eq!(
                        *m.get(&Value::String("bool_val".to_string())),
                        Value::Bool(true)
                    );
                }
                _ => panic!("Expected Map"),
            }
        }
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_exec_decode_column_and_value_positions_align() {
    let row = MockRow::new(
        vec!["first", "second", "third"],
        vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
            Value::String("c".to_string()),
        ],
    );
    let conn = MockConnection::with_rows(vec![Box::new(row)]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT first, second, third FROM t");
    let value = result.unwrap();

    if let Value::Array(arr) = value {
        assert_eq!(arr.len(), 1);

        if let Value::Map(m) = &arr[0] {
            assert_eq!(
                *m.get(&Value::String("first".to_string())),
                Value::String("a".to_string())
            );
            assert_eq!(
                *m.get(&Value::String("second".to_string())),
                Value::String("b".to_string())
            );
            assert_eq!(
                *m.get(&Value::String("third".to_string())),
                Value::String("c".to_string())
            );
        } else {
            panic!("Expected Map");
        }
    } else {
        panic!("Expected Array");
    }
}

#[test]
fn test_exec_decode_correct_length_fields() {
    let row = MockRow::new(
        vec!["a", "b", "c", "d", "e"],
        vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
            Value::I64(4),
            Value::I64(5),
        ],
    );
    let conn = MockConnection::with_rows(vec![Box::new(row)]);
    let mut conn = conn;
    let result = exec_decode_sync(&mut conn, "SELECT a, b, c, d, e FROM t");
    let value = result.unwrap();

    if let Value::Array(arr) = value {
        assert_eq!(arr.len(), 1);
        if let Value::Map(m) = &arr[0] {
            assert_eq!(m.len(), 5);
        } else {
            panic!("Expected Map");
        }
    } else {
        panic!("Expected Array");
    }
}
