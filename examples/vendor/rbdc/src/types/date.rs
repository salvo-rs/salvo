use crate::Error;
use fastdate::DateTime;
use rbs::Value;
use std::fmt::{Debug, Display, Formatter};
use std::str::FromStr;

#[derive(serde::Serialize, serde::Deserialize, Clone, Eq, PartialEq, Hash)]
#[serde(rename = "Date")]
pub struct Date(pub fastdate::Date);

impl Display for Date {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Debug for Date {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Date({})", self.0)
    }
}

impl From<Date> for Value {
    fn from(arg: Date) -> Self {
        Value::Ext("Date", Box::new(Value::String(arg.0.to_string())))
    }
}

impl FromStr for Date {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Date(
            fastdate::Date::from_str(s).map_err(|e| Error::from(e.to_string()))?,
        ))
    }
}

impl From<Date> for fastdate::Date {
    fn from(value: Date) -> Self {
        value.0
    }
}

impl From<DateTime> for Date {
    fn from(value: DateTime) -> Self {
        Date(value.into())
    }
}

impl Default for Date {
    fn default() -> Self {
        Date(fastdate::Date {
            day: 1,
            mon: 1,
            year: 1970,
        })
    }
}
