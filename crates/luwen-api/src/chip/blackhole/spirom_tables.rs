use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub mod flash_info {
    include!(concat!(env!("OUT_DIR"), "/flash_info.rs"));
}
pub mod fw_table {
    include!(concat!(env!("OUT_DIR"), "/fw_table.rs"));
}
pub mod read_only {
    include!(concat!(env!("OUT_DIR"), "/read_only.rs"));
}

pub fn remove_padding_proto_bin(proto_bin: &[u8]) -> Result<&[u8], Box<dyn std::error::Error>> {
    // The proto bins have to be padded to be a multiple of 4 bytes to fit into the spirom requirements
    // This means that we have to read the last byte of the bin and remove num + 1 num of bytes
    // 0: remove 1 byte (0)
    // 1: remove 2 bytes (0, 1)
    // 2: remove 3 bytes (0, X, 2)
    // 3: remove 4 bytes (0, X, X, 3)

    // Ensure the input slice is not empty
    if proto_bin.is_empty() {
        return Err("Input slice is empty".into());
    }
    // Some FW versions (80.18.0 - 18.5.0) have a bug padding proto_bins to a multiple of 8 bytes
    // using trailing 0s. Valid encoded protobufs shouldn't have extra 0s after the padding
    let proto_bin = if proto_bin.len() % 8 == 0 && proto_bin.ends_with(&[0, 0, 0, 0]) {
        &proto_bin[..proto_bin.len() - 4]
    } else {
        proto_bin
    };
    let last_byte = proto_bin[proto_bin.len() - 1] as usize;
    // Ensure the input slice has enough bytes to remove the padding
    if proto_bin.len() < last_byte + 1 {
        return Err("Input slice is too short to remove padding".into());
    }
    // truncate the last byte and the padding bytes
    Ok(&proto_bin[..proto_bin.len() - last_byte - 1])
}

// Generic function to convert any serializable type into a HashMap
pub fn to_hash_map<T: Serialize>(value: T) -> HashMap<String, Value> {
    // Serialize the value to JSON
    let json_string = serde_json::to_string(&value).unwrap();
    // Deserialize the JSON into a HashMap
    serde_json::from_str(&json_string).unwrap()
}

// Generic function to convert a HashMap into a deserializable type
pub fn from_hash_map<T: DeserializeOwned>(map: HashMap<String, Value>) -> T {
    // Serialize the HashMap to JSON
    let json_string = serde_json::to_string(&map).unwrap();
    // Deserialize the JSON into the desired type
    serde_json::from_str(&json_string).unwrap()
}

pub fn calculate_checksum(data: &[u8]) -> u32 {
    // The input slice `data` must have a size that is a multiple of 4 bytes
    // to safely convert chunks into `u32` values.
    let mut calculate_checksum: u32 = 0;
    for i in (0..data.len()).step_by(4) {
        let value = u32::from_le_bytes(data[i..i + 4].try_into().unwrap());
        // Do a wrapping add to prevent overflow
        calculate_checksum = calculate_checksum.wrapping_add(value);
    }
    calculate_checksum
}
