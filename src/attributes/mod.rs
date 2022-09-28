mod attributes_list;
mod data;
mod file_name;
mod standard_info;

use chrono::{DateTime, Duration, NaiveDate, Utc};

pub use attributes_list::{AttributeList, AttributeListItem};
pub use data::Data;
pub use file_name::FileName;
pub use standard_info::StandardInformation;

// https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime
// Contains a 64-bit value representing the number of 100-nanosecond intervals since January 1, 1601 (UTC).
pub fn convert_u64_to_datetime(timestamp: u64) -> DateTime<Utc> {
    // From 1/1/1601 00:00:00.0 add timestamp as microseconds
    DateTime::from_utc(
        NaiveDate::from_ymd(1601, 1, 1).and_hms_nano(0, 0, 0, 0)
            + Duration::microseconds((timestamp / 10) as i64),
        Utc,
    )
}

#[cfg(test)]
mod iterator_tests {
    use super::*;
    #[test]
    fn timestamp_test() {
        let data: u64 = 0x989680;
        let date = convert_u64_to_datetime(data);
        assert_eq!(
            date,
            DateTime::<Utc>::from_utc(NaiveDate::from_ymd(1601, 1, 1).and_hms(0, 0, 1), Utc)
        );
    }
}
