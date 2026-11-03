use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

use super::Status;

// The legacy `Status` layout stays first so its bytes match `Response::Status`.
#[derive(Serialize)]
struct WireOut<'a> {
    status: &'a Status,
    total_files: Vec<Option<u64>>,
}

#[derive(Deserialize)]
struct WireIn {
    status: Status,
    total_files: Vec<Option<u64>>,
}

pub(super) fn serialize<S: Serializer>(status: &Status, serializer: S) -> Result<S::Ok, S::Error> {
    WireOut {
        status,
        total_files: status
            .volumes
            .iter()
            .map(|volume| volume.total_files)
            .collect(),
    }
    .serialize(serializer)
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Status, D::Error> {
    let WireIn {
        mut status,
        total_files,
    } = WireIn::deserialize(deserializer)?;
    if total_files.len() != status.volumes.len() {
        return Err(D::Error::custom("status totals do not match its volumes"));
    }
    for (volume, total) in status.volumes.iter_mut().zip(total_files) {
        volume.total_files = total;
        volume.clamp_total();
    }
    Ok(status)
}
