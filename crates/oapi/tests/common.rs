use serde_json::Value;

pub fn value_as_string(value: Option<&'_ Value>) -> String {
    value.unwrap_or(&Value::Null).to_string()
}

#[allow(unused)]
pub fn assert_json_array_len(value: &Value, len: usize) {
    match value {
        Value::Array(array) => assert_eq!(
            len,
            array.len(),
            "wrong amount of parameters {} != {}",
            len,
            array.len()
        ),
        _ => unreachable!(),
    }
}

#[macro_export]
macro_rules! assert_value {
    ($value:expr=> $( $path:literal = $expected:literal, $error:literal)* ) => {{
        $(
            let p = &*format!("/{}", $path.replace(".", "/").replace("[", "").replace("]", ""));
            let actual = $crate::common::value_as_string(Some($value.pointer(p).unwrap_or(&serde_json::Value::Null)));
            assert_eq!(actual, $expected, "{}: {} expected to be: {} but was: {}", $error, $path, $expected, actual);
         )*
    }};

    ($value:expr=> $( $path:literal = $expected:expr, $error:literal)*) => {
        {
            $(
                let p = &*format!("/{}", $path.replace(".", "/").replace("[", "").replace("]", ""));
                let actual = $value.pointer(p).unwrap_or(&serde_json::Value::Null);
                assert!(actual == &$expected, "{}: {} expected to be: {:?} but was: {:?}", $error, $path, $expected, actual);
             )*
        }
    }
}
