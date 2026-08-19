//! SystemTime as (secs, nanos) since UNIX_EPOCH for JSON documents.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(serde::ser::Error::custom)?;
    (duration.as_secs(), duration.subsec_nanos()).serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
where
    D: Deserializer<'de>,
{
    let (secs, nanos) = <(u64, u32)>::deserialize(deserializer)?;
    Ok(UNIX_EPOCH + Duration::new(secs, nanos))
}
