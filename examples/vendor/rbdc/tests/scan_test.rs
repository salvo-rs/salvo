use futures_util::stream;
use rbdc::Error;
use rbdc::db::{MetaData, Row};
use rbdc::util::Scan;
use rbs::Value;
use std::fmt::Debug;

struct MockMetaData {
    columns: Vec<String>,
}

impl MockMetaData {
    fn new(columns: Vec<String>) -> Self {
        Self { columns }
    }
}

impl Debug for MockMetaData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockMetaData").finish()
    }
}

impl MetaData for MockMetaData {
    fn column_len(&self) -> usize {
        self.columns.len()
    }

    fn column_name(&self, i: usize) -> String {
        self.columns.get(i).cloned().unwrap_or_default()
    }

    fn column_type(&self, _i: usize) -> String {
        String::new()
    }
}

struct MockRow {
    columns: Vec<String>,
    values: Vec<Value>,
}

impl MockRow {
    fn new(columns: Vec<String>, values: Vec<Value>) -> Self {
        Self { columns, values }
    }
}

impl Debug for MockRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockRow").finish()
    }
}

impl Row for MockRow {
    fn meta_data(&self) -> Box<dyn MetaData> {
        Box::new(MockMetaData::new(self.columns.clone()))
    }

    fn get(&mut self, i: usize) -> Result<Value, Error> {
        self.values
            .get(i)
            .cloned()
            .ok_or_else(|| Error::from("index out of bounds"))
    }
}

#[tokio::test]
async fn test_scan_stream_collect() {
    let rows: Vec<Box<dyn Row>> = vec![
        Box::new(MockRow::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::I64(1), Value::String("a".into())],
        )),
        Box::new(MockRow::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::I64(2), Value::String("b".into())],
        )),
    ];
    let stream = stream::iter(rows.into_iter().map(Ok::<Box<dyn Row>, Error>));
    let scan = Scan::new(stream);

    let values: Vec<Value> = scan.collect::<Value>().await.unwrap();
    assert_eq!(values.len(), 2);
    match &values[0] {
        Value::Map(_map) => {
            // Map contains key-value pairs
            assert!(true);
        }
        _ => panic!("expected Map"),
    }
}

#[tokio::test]
async fn test_scan_empty() {
    let rows: Vec<Box<dyn Row>> = vec![];
    let stream = stream::iter(rows.into_iter().map(Ok::<Box<dyn Row>, Error>));
    let scan = Scan::new(stream);

    let values: Vec<Value> = scan.collect::<Value>().await.unwrap();
    assert!(values.is_empty());
}

#[test]
fn test_scan_debug() {
    let rows: Vec<Box<dyn Row>> = vec![Box::new(MockRow::new(
        vec!["id".to_string()],
        vec![Value::I64(1)],
    ))];
    let stream = stream::iter(rows.into_iter().map(Ok::<Box<dyn Row>, Error>));
    let scan = Scan::new(stream);
    let debug_str = format!("{:?}", scan);
    assert!(debug_str.contains("Scan"));
}

#[tokio::test]
async fn test_scan_collect_stream_item_error() {
    let rows = vec![Err::<Box<dyn Row>, Error>(Error::from("boom"))];
    let scan = Scan::new(stream::iter(rows));
    let result: Result<Vec<Value>, Error> = scan.collect().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_scan_collect_deserialize_error() {
    #[derive(serde::Deserialize)]
    struct Strict {
        #[allow(dead_code)]
        id: i32,
    }

    let rows: Vec<Result<Box<dyn Row>, Error>> = vec![Ok(Box::new(MockRow::new(
        vec!["id".to_string()],
        vec![Value::String("not-an-int".to_string())],
    )))];
    let scan = Scan::new(stream::iter(rows));
    let result: Result<Vec<Strict>, Error> = scan.collect().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_scan_stream_impl() {
    use futures_util::StreamExt;

    let rows: Vec<Result<Box<dyn Row>, Error>> = vec![
        Ok(Box::new(MockRow::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::I64(1), Value::String("a".to_string())],
        ))),
        Ok(Box::new(MockRow::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::I64(2), Value::String("b".to_string())],
        ))),
    ];
    let scan = Scan::new(stream::iter(rows));
    let collected: Vec<Result<Value, Error>> = StreamExt::collect(scan).await;
    assert_eq!(collected.len(), 2);
    assert!(matches!(collected[0], Ok(Value::Map(_))));

    // Errors propagated through the Stream impl.
    let rows: Vec<Result<Box<dyn Row>, Error>> = vec![Err(Error::from("stream error"))];
    let scan = Scan::new(stream::iter(rows));
    let collected: Vec<Result<Value, Error>> = StreamExt::collect(scan).await;
    assert_eq!(collected.len(), 1);
    assert!(collected[0].is_err());
}
