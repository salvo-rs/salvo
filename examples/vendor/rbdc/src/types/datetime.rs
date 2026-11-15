use crate::Error;
use crate::date::Date;
use crate::types::time::Time;
use rbs::Value;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Add, Sub};
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct DateTime(pub fastdate::DateTime);

impl Display for DateTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for DateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_newtype_struct("DateTime", &self.0)
    }
}

impl Debug for DateTime {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DateTime({})", self.0)
    }
}

impl<'de> Deserialize<'de> for DateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        #[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
        #[serde(rename = "DateTime")]
        pub struct DateTimeValue(pub Value);
        let v = DateTimeValue::deserialize(deserializer)?;
        match v.0 {
            Value::I32(u) => Ok(Self(fastdate::DateTime::from_timestamp_millis(u as i64))),
            Value::U32(u) => Ok(Self(fastdate::DateTime::from_timestamp_millis(u as i64))),
            Value::I64(u) => Ok(Self(fastdate::DateTime::from_timestamp_millis(u))),
            Value::U64(u) => Ok(Self(fastdate::DateTime::from_timestamp_millis(u as i64))),
            Value::String(s) => Ok({
                Self(
                    fastdate::DateTime::from_str(&s)
                        .map_err(|e| D::Error::custom(e.to_string()))?,
                )
            }),
            _ => {
                return Err(D::Error::custom(&format!(
                    "unsupported type DateTime({})",
                    v.0
                )));
            }
        }
    }
}

impl DateTime {
    pub fn now() -> Self {
        Self(fastdate::DateTime::now())
    }

    pub fn utc() -> Self {
        Self(fastdate::DateTime::utc())
    }

    /// set offset
    /// ```rust
    /// let mut  dt = rbdc::types::datetime::DateTime::utc();
    /// dt = dt.set_offset(fastdate::offset_sec());
    /// ```
    pub fn set_offset(self, offset_sec: i32) -> DateTime {
        Self(self.0.set_offset(offset_sec))
    }

    pub fn add(self, d: Duration) -> Self {
        Self(self.0.add(d))
    }

    pub fn sub(self, d: Duration) -> Self {
        Self(self.0.sub(d))
    }

    pub fn add_sub_sec(self, sec: i64) -> Self {
        Self(self.0.add_sub_sec(sec))
    }

    pub fn before(&self, other: &DateTime) -> bool {
        self.0.before(&other.0)
    }

    pub fn after(&self, other: &DateTime) -> bool {
        self.0.after(&other.0)
    }

    pub fn unix_timestamp(&self) -> i64 {
        self.0.unix_timestamp()
    }

    pub fn unix_timestamp_micros(&self) -> i64 {
        self.0.unix_timestamp_micros()
    }

    pub fn unix_timestamp_millis(&self) -> i64 {
        self.0.unix_timestamp_millis()
    }

    pub fn unix_timestamp_nano(&self) -> i128 {
        self.0.unix_timestamp_nano()
    }

    pub fn from_timestamp(sec: i64) -> Self {
        DateTime(fastdate::DateTime::from_timestamp(sec))
    }

    pub fn from_timestamp_micros(micros: i64) -> DateTime {
        DateTime(fastdate::DateTime::from_timestamp_micros(micros))
    }
    pub fn from_timestamp_millis(ms: i64) -> Self {
        DateTime(fastdate::DateTime::from_timestamp_millis(ms))
    }

    pub fn from_timestamp_nano(nano: i128) -> Self {
        DateTime(fastdate::DateTime::from_timestamp_nano(nano))
    }

    /// format support token = ["YYYY","MM","DD","hh","mm","ss",".000000",".000000000","+00:00"]
    /// ```
    ///   let dt = rbdc::DateTime::now();
    ///   println!("{}",dt.format("YYYY-MM-DD hh:mm:ss"));
    ///   println!("{}",dt.format("YYYY-MM-DD hh:mm:ss.000000"));
    ///   println!("{}",dt.format("YYYY/MM/DD/hh/mm/ss/.000000/+00:00"));
    ///   println!("{}",dt.format("YYYY-MM-DD/hh/mm/ss"));
    /// ```
    pub fn format(&self, fmt: &str) -> String {
        self.0.format(fmt)
    }

    /// parse an string by format.
    /// format str must be:
    /// ```rust
    ///  rbdc::types::datetime::DateTime::parse("YYYY-MM-DD hh:mm:ss.000000","2022-12-13 11:12:14.123456").unwrap();
    /// ```
    /// or any position
    /// ```rust
    ///  rbdc::types::datetime::DateTime::parse("YYYY-MM-DD hh:mm:ss.000000","2022-12-13 11:12:14.123456").unwrap();
    /// ```
    pub fn parse(format: &str, arg: &str) -> Result<DateTime, Error> {
        Ok(Self(
            fastdate::DateTime::parse(format, arg).map_err(|e| Error::from(e.to_string()))?,
        ))
    }

    pub fn week_day(&self) -> u8 {
        self.0.week_day()
    }
    pub fn nano(&self) -> u32 {
        self.0.nano()
    }

    pub fn ms(&self) -> u16 {
        self.0.ms()
    }

    pub fn micro(&self) -> u32 {
        self.0.micro()
    }

    pub fn sec(&self) -> u8 {
        self.0.sec()
    }

    /// minute
    pub fn minute(&self) -> u8 {
        self.0.minute()
    }

    /// get hour
    pub fn hour(&self) -> u8 {
        self.0.hour()
    }

    /// get day
    pub fn day(&self) -> u8 {
        self.0.day()
    }

    pub fn mon(&self) -> u8 {
        self.0.mon()
    }

    /// get year
    pub fn year(&self) -> i32 {
        self.0.year()
    }

    ///offset sec
    pub fn offset(&self) -> i32 {
        self.0.offset()
    }

    pub fn from_system_time(s: SystemTime, offset: i32) -> Self {
        Self(fastdate::DateTime::from_system_time(s, offset))
    }

    /// stand "0000-00-00 00:00:00.000000000"
    pub fn display_stand(&self) -> String {
        self.0.display_stand()
    }

    /// RFC3339 "0000-00-00T00:00:00.000000000Z"
    /// RFC3339 "0000-00-00T00:00:00.000000000+00:00:00"
    pub fn display(&self, zone: bool) -> String {
        self.0.display(zone)
    }

    /// let mut buf: [u8; 38] = *b"0000-00-00T00:00:00.000000000+00:00:00";
    /// than print this:
    /// RFC3339 "0000-00-00T00:00:00.000000000Z"
    /// RFC3339 "0000-00-00T00:00:00.000000000+00:00:00"
    pub fn do_display(&self, buf: &mut [u8; 38], add_zone: bool) -> usize {
        self.0.do_display(buf, add_zone)
    }

    pub fn set_nano(self, nano: u32) -> Self {
        Self(self.0.set_nano(nano))
    }
}

impl Add<Duration> for DateTime {
    type Output = DateTime;

    fn add(self, rhs: Duration) -> Self::Output {
        DateTime(self.0.add(rhs))
    }
}

impl Sub<Duration> for DateTime {
    type Output = DateTime;

    fn sub(self, rhs: Duration) -> Self::Output {
        DateTime(self.0.sub(rhs))
    }
}

impl Add<&Duration> for DateTime {
    type Output = DateTime;

    fn add(self, rhs: &Duration) -> Self::Output {
        Self(self.0.add(rhs.clone()))
    }
}

impl Sub<&Duration> for DateTime {
    type Output = DateTime;

    fn sub(self, rhs: &Duration) -> Self::Output {
        Self(self.0.sub(rhs.clone()))
    }
}

impl Sub<DateTime> for DateTime {
    type Output = Duration;

    fn sub(self, rhs: DateTime) -> Self::Output {
        let nano = self.unix_timestamp_nano() - rhs.unix_timestamp_nano();
        Duration::from_nanos(nano as u64)
    }
}

impl From<SystemTime> for DateTime {
    fn from(v: SystemTime) -> DateTime {
        DateTime::from_system_time(v, 0)
    }
}

impl From<DateTime> for SystemTime {
    fn from(v: DateTime) -> SystemTime {
        let nano = v.unix_timestamp_nano();
        if nano >= 0 {
            UNIX_EPOCH + Duration::from_nanos(nano as u64)
        } else {
            // Use the magnitude: casting a negative i128 with `as u64` would
            // produce a huge duration and overflow on platforms whose
            // SystemTime cannot represent dates before the Unix epoch (Windows).
            UNIX_EPOCH - Duration::from_nanos(nano.unsigned_abs() as u64)
        }
    }
}

impl From<Date> for DateTime {
    fn from(arg: Date) -> Self {
        Self(fastdate::DateTime::from(arg.0))
    }
}

impl From<Time> for DateTime {
    fn from(arg: Time) -> Self {
        Self(fastdate::DateTime::from(arg.0))
    }
}

impl From<(Date, Time)> for DateTime {
    fn from(arg: (Date, Time)) -> Self {
        Self(fastdate::DateTime::from((arg.0.0, arg.1.0)))
    }
}

impl From<(Date, Time, i32)> for DateTime {
    fn from(arg: (Date, Time, i32)) -> Self {
        Self(fastdate::DateTime::from((arg.0.0, arg.1.0, arg.2)))
    }
}

impl FromStr for DateTime {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DateTime(
            fastdate::DateTime::from_str(s)
                .map_err(|e| crate::error::Error::from(e.to_string()))?,
        ))
    }
}

impl From<DateTime> for Value {
    fn from(arg: DateTime) -> Self {
        Value::Ext("DateTime", Box::new(Value::String(arg.0.to_string())))
    }
}

impl Ord for DateTime {
    fn cmp(&self, other: &DateTime) -> cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for DateTime {
    fn partial_cmp(&self, other: &DateTime) -> Option<cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl From<DateTime> for fastdate::DateTime {
    fn from(value: DateTime) -> Self {
        value.0
    }
}

impl Default for DateTime {
    fn default() -> Self {
        DateTime(fastdate::DateTime::from_timestamp(0))
    }
}

#[cfg(test)]
mod test {
    use crate::datetime::DateTime;
    use std::str::FromStr;

    #[test]
    fn test_ser_de() {
        let dt = DateTime::now();
        let v = serde_json::to_value(&dt).unwrap();
        println!("{}", v);
        let new_dt: DateTime = serde_json::from_value(v).unwrap();
        assert_eq!(new_dt, dt);
    }

    #[test]
    fn test_de() {
        let dt = DateTime::from_str("2023-10-21T00:15:00.9233333+08:00").unwrap();
        println!("dt={}", dt);
        let v = serde_json::to_value(&dt).unwrap();
        let new_dt: DateTime = serde_json::from_value(v).unwrap();
        assert_eq!(new_dt, dt);
    }

    #[test]
    fn test_de2() {
        let dt = vec![DateTime::from_str("2023-10-21T00:15:00.9233333+08:00").unwrap()];
        let v = serde_json::to_value(&dt).unwrap();
        println!("dt={:?}", dt);
        let new_dt: Vec<DateTime> = serde_json::from_value(v).unwrap();
        assert_eq!(new_dt, dt);
    }

    #[test]
    fn test_de3() {
        let dt = vec![DateTime::from_str("2023-10-21T00:15:00.9233333+08:00").unwrap()];
        let v = rbs::value!(&dt);
        let new_dt: Vec<DateTime> = rbs::from_value(v).unwrap();
        assert_eq!(new_dt, dt);
    }

    #[test]
    fn test_de4() {
        let dt = DateTime::from_str("2023-10-21T00:15:00.9233333+08:00").unwrap();
        let v = rbs::value!(&dt.unix_timestamp_millis());
        let new_dt: DateTime = rbs::from_value(v).unwrap();
        assert_eq!(
            new_dt,
            DateTime::from_str("2023-10-20T16:15:00.923Z").unwrap()
        );
    }

    #[test]
    fn test_de5() {
        let dt = DateTime::from_str("2023-10-21T00:15:00.9233333+08:00").unwrap();
        let v = serde_json::to_value(&dt.unix_timestamp_millis()).unwrap();
        let new_dt: DateTime = serde_json::from_value(v).unwrap();
        assert_eq!(
            new_dt,
            DateTime::from_str("2023-10-20T16:15:00.923Z").unwrap()
        );
    }

    #[test]
    fn test_default() {
        let dt = DateTime::default();
        println!("{}", dt);
        assert_eq!(dt.to_string(), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn test_format() {
        let dt = DateTime::default();
        let s = dt.format("YYYY-MM-DD/hh/mm/ss");
        println!("{}", s);
        assert_eq!(s, "1970-01-01/00/00/00");
    }
}
