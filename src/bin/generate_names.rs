use std::collections::HashMap;

use serde::{
    de::{value::SeqAccessDeserializer, Visitor},
    Deserialize, Deserializer, Serialize,
};
use ttchip::axi::MemorySlice;

#[derive(Debug, Serialize)]
pub enum Fields {
    Array {
        mask: u32,
        upper_bits: u32,
        lower_bits: u32,
        description: String,
    },
    Struct {
        mask: u32,
        upper_bits: u32,
        lower_bits: u32,
        byte_offset: u32,
        description: String,
    },
}

impl<'de> Deserialize<'de> for Fields {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DeserializeFields;

        impl<'de> Visitor<'de> for DeserializeFields {
            type Value = Fields;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a list either either 4 or 5 elements long")
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size = seq.size_hint().unwrap();

                if size == 4 {
                    let value: (u32, u32, u32, String) =
                        Deserialize::deserialize(SeqAccessDeserializer::new(seq))?;

                    Ok(Fields::Array {
                        mask: value.0,
                        upper_bits: value.1,
                        lower_bits: value.2,
                        description: value.3,
                    })
                } else if size == 5 {
                    let value: (u32, u32, u32, u32, String) =
                        Deserialize::deserialize(SeqAccessDeserializer::new(seq))?;

                    Ok(Fields::Struct {
                        mask: value.0,
                        upper_bits: value.1,
                        lower_bits: value.2,
                        byte_offset: value.3,
                        description: value.4,
                    })
                } else {
                    Err(serde::de::Error::invalid_length(size, &self))
                }
            }
        }

        deserializer.deserialize_any(DeserializeFields)
    }
}

fn deserialize_fields<'de, D>(fields: D) -> Result<HashMap<String, Fields>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct DeserializeFieldMap;

    impl<'de> Visitor<'de> for DeserializeFieldMap {
        type Value = HashMap<String, Fields>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map of strings to either either 4 or 5 element lists")
        }

        fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::MapAccess<'de>,
        {
            let mut output = HashMap::new();
            while let Some((key, value)) = map.next_entry()? {
                output.insert(key, value);
            }

            Ok(output)
        }
    }

    fields.deserialize_any(DeserializeFieldMap)
}

#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryRegion {
    #[serde(rename = "Address")]
    pub address: u64,
    #[serde(rename = "ArraySize")]
    pub array_size: Option<u32>,
    #[serde(rename = "AddressIncrement")]
    pub address_increment: Option<u32>,
    #[serde(rename = "Description")]
    pub description: Option<String>,
    #[serde(rename = "Fields", deserialize_with = "deserialize_fields")]
    pub fields: HashMap<String, Fields>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryDefs {
    #[serde(rename = "Regsize")]
    pub regsize: u64,
    #[serde(flatten)]
    pub regions: HashMap<String, MemoryRegion>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryTopLevel {
    pub offset: u64,
    pub filename: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryFile {
    #[serde(flatten)]
    pub tops: HashMap<String, MemoryTopLevel>,
}

fn parse_translation_file(
    path: &str,
    file: &str,
) -> Result<HashMap<String, MemorySlice>, Box<dyn std::error::Error>> {
    let top_level: MemoryFile = serde_yaml::from_slice(&std::fs::read(&format!("{path}/{file}"))?)?;

    let mut slices = HashMap::with_capacity(top_level.tops.len());
    for (name, top) in top_level.tops {
        println!("Parsing {name}");
        let defs: MemoryDefs =
            serde_yaml::from_slice(&std::fs::read(&format!("{path}/{}", top.filename))?)?;

        let value = slices.entry(name.clone()).or_insert_with(|| MemorySlice {
            name,
            offset: top.offset,
            size: 0,
            array_count: None,
            bit_mask: None,
            children: HashMap::with_capacity(defs.regions.len()),
        });

        for (region_name, def) in defs.regions {
            let all_array = def.fields.values().all(|f| {
                if let Fields::Array { .. } = f {
                    true
                } else {
                    false
                }
            });
            let all_struct = def.fields.values().all(|f| {
                if let Fields::Struct { .. } = f {
                    true
                } else {
                    false
                }
            });
            assert!(all_array || all_struct);

            let address_increment = def.address_increment.map(|v| v as u64).unwrap_or(defs.regsize / 8);

            let mut slice = if all_array {
                MemorySlice {
                    name: region_name.clone(),
                    offset: def.address,
                    size: address_increment,
                    array_count: def.array_size.map(|v| v as u64),
                    bit_mask: None,
                    children: HashMap::with_capacity(def.fields.len()),
                }
            } else {
                MemorySlice {
                    name: region_name.clone(),
                    offset: def.address,
                    size: address_increment * def.array_size.map(|v| v as u64).unwrap_or(1),
                    array_count: None,
                    bit_mask: None,
                    children: HashMap::with_capacity(def.fields.len()),
                }
            };
            for (field_name, field) in def.fields {
                match field {
                    Fields::Array {
                        mask,
                        upper_bits,
                        lower_bits,
                        description: _,
                    } => {
                        // assert_eq!(mask, 0);

                        slice.children.insert(
                            field_name.clone(),
                            MemorySlice {
                                name: field_name,
                                offset: 0,
                                size: 0,
                                array_count: None,
                                bit_mask: Some((lower_bits as u64, upper_bits as u64)),
                                children: HashMap::new(),
                            },
                        );
                    }
                    Fields::Struct {
                        mask,
                        upper_bits,
                        lower_bits,
                        byte_offset,
                        description: _,
                    } => {
                        // assert_eq!(mask, 0);

                        slice.children.insert(
                            field_name.clone(),
                            MemorySlice {
                                name: field_name,
                                offset: byte_offset as u64,
                                size: 0,
                                array_count: None,
                                bit_mask: Some((lower_bits as u64, upper_bits as u64)),
                                children: HashMap::new(),
                            },
                        );
                    }
                }
            }
            value.children.insert(region_name, slice);
        }
    }

    Ok(slices)
}

fn parse_and_serialize_translation(
    path: &str,
    file: &str,
    output: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = bincode::serialize(&parse_translation_file(path, file)?)?;
    std::fs::write(output, data)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    parse_and_serialize_translation("data/grayskull", "axi-pci.yaml", "grayskull-axi-pci.bin")?;
    parse_and_serialize_translation("data/wormhole", "axi-pci.yaml", "wormhole-axi-pci.bin")?;

    Ok(())
}
