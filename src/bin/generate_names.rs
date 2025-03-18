// SPDX-FileCopyrightText: © 2023 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use luwen_if::chip::{axi_translate, MemorySlice, MemorySlices};
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, one_of},
    combinator::{all_consuming, map_res, recognize},
    multi::{many0, many1},
    sequence::{preceded, terminated},
    Finish, IResult,
};
use serde::{
    de::{value::SeqAccessDeserializer, Visitor},
    Deserialize, Deserializer, Serialize,
};

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
        byte_offset: u64,
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
                    let value: (u32, u32, u32, u64, String) =
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

fn parse_hexidecimal(i: &str) -> IResult<&str, u64> {
    map_res(
        preceded(
            alt((tag("0x"), tag("0X"))),
            recognize(many1(terminated(
                one_of("0123456789abcdefABCDEF"),
                many0(char('_')),
            ))),
        ),
        |value: &str| u64::from_str_radix(&value.replace('_', ""), 16),
    )(i)
}

fn parse_decimal(i: &str) -> IResult<&str, u64> {
    map_res(
        recognize(many1(terminated(one_of("0123456789"), many0(char('_'))))),
        #[allow(clippy::from_str_radix_10)]
        |value: &str| u64::from_str_radix(&value.replace('_', ""), 10),
    )(i)
}

fn parse_octal(i: &str) -> IResult<&str, u64> {
    map_res(
        preceded(
            alt((tag("0o"), tag("0O"))),
            recognize(many1(terminated(one_of("01234567"), many0(char('_'))))),
        ),
        |value: &str| u64::from_str_radix(&value.replace('_', ""), 8),
    )(i)
}

fn parse_binary(i: &str) -> IResult<&str, u64> {
    map_res(
        preceded(
            alt((tag("0b"), tag("0B"))),
            recognize(many1(terminated(one_of("01"), many0(char('_'))))),
        ),
        |value: &str| u64::from_str_radix(&value.replace('_', ""), 2),
    )(i)
}

fn deserialize_underscore_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: serde_yaml::Value = Deserialize::deserialize(deserializer)?;
    let s = serde_yaml::to_string(&s)
        .map_err(|_| serde::de::Error::custom("Failed to convert yaml value to string"))?;

    let s = s.trim();

    // Then use nom to parse the string as an integer, this parser accepts (and ignores) '_' in the
    // value.
    let (_, s) = all_consuming(alt((
        parse_hexidecimal,
        parse_decimal,
        parse_binary,
        parse_octal,
    )))(s)
    .finish()
    .map_err(|e| serde::de::Error::custom(format!("Could not parse string {s} as integer: {e}")))?;

    Ok(s)
}

#[derive(Default, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MemoryRegion {
    #[serde(rename = "Address", deserialize_with = "deserialize_underscore_number")]
    pub address: u64,
    #[serde(rename = "ArraySize")]
    pub array_size: Option<u64>,
    #[serde(rename = "AddressIncrement")]
    pub address_increment: Option<u64>,
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
    #[serde(deserialize_with = "deserialize_underscore_number")]
    pub offset: u64,
    pub filename: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MemoryFile {
    #[serde(flatten)]
    pub tops: HashMap<String, MemoryTopLevel>,
}

fn parse_yaml_translation_file(
    path: &str,
    file: &str,
) -> Result<HashMap<String, MemorySlice>, Box<dyn std::error::Error>> {
    let top_level: MemoryFile = serde_yaml::from_slice(&std::fs::read(format!("{path}/{file}"))?)?;

    let mut slices = HashMap::with_capacity(top_level.tops.len());
    for (name, top) in top_level.tops {
        println!("Parsing {name}");
        let defs: MemoryDefs =
            serde_yaml::from_slice(&std::fs::read(format!("{path}/{}", top.filename))?)?;

        let value = slices.entry(name.clone()).or_insert_with(|| MemorySlice {
            name,
            offset: top.offset,
            size: 0,
            array_count: None,
            bit_mask: None,
            children: HashMap::with_capacity(defs.regions.len()),
        });

        for (region_name, def) in defs.regions {
            let all_array = def
                .fields
                .values()
                .all(|f| matches!(f, Fields::Array { .. }));
            let all_struct = def
                .fields
                .values()
                .all(|f| matches!(f, Fields::Struct { .. }));
            assert!(
                def.fields.is_empty() || (all_array ^ all_struct),
                "{} ^ {} from {:?}",
                all_array,
                all_struct,
                def.fields
            );

            let address_increment = def.address_increment.unwrap_or(defs.regsize / 8);

            let mut slice = if all_array {
                MemorySlice {
                    name: region_name.clone(),
                    offset: def.address,
                    size: address_increment,
                    array_count: def.array_size,
                    bit_mask: None,
                    children: HashMap::with_capacity(def.fields.len()),
                }
            } else {
                MemorySlice {
                    name: region_name.clone(),
                    offset: def.address,
                    size: address_increment * def.array_size.unwrap_or(1),
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
                        if mask != 0 {
                            println!("WARNING: while parsing {field_name} found non zero field info[0] {mask}");
                        }

                        let size = ((upper_bits + 1 + 7) / 8) as u64;

                        slice.children.insert(
                            field_name.clone(),
                            MemorySlice {
                                name: field_name,
                                offset: 0,
                                size,
                                array_count: None,
                                bit_mask: Some((lower_bits, upper_bits)),
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
                        if mask != 0 {
                            println!("WARNING: while parsing {field_name} found non zero field info[0] {mask}");
                        }

                        let size = ((upper_bits + 1 + 7) / 8) as u64;

                        slice.children.insert(
                            field_name.clone(),
                            MemorySlice {
                                name: field_name,
                                offset: byte_offset,
                                size,
                                array_count: None,
                                bit_mask: Some((lower_bits, upper_bits)),
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

#[allow(dead_code)]
fn parse_yaml_and_serialize_translation(
    path: &str,
    file: &str,
    output: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let data = parse_yaml_translation_file(path, file)?;
    std::fs::write(output, bincode::serialize(&MemorySlices::Tree(data))?)?;

    Ok(())
}

fn parse_yaml_and_serialize_translation_singlelayer(
    path: &str,
    file: &str,
    output: &str,
    whitelist: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let data = MemorySlices::Tree(parse_yaml_translation_file(path, file)?);

    let mut output_data = HashMap::new();
    for w in whitelist {
        output_data.insert(w.to_string(), axi_translate(Some(&data), w)?);
    }

    let data = bincode::serialize(&MemorySlices::Flat(output_data))?;
    std::fs::write(output, data)?;

    Ok(())
}

fn maybe_deserialize_rdl_number<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: serde_json::Value = Deserialize::deserialize(deserializer)?;

    Ok(if s.is_object() {
        let number = s.get("int").unwrap().as_str().unwrap();

        // Then use nom to parse the string as an integer, this parser accepts (and ignores) '_' in the
        // value.
        let (_, s) = all_consuming(alt((
            parse_hexidecimal,
            parse_decimal,
            parse_binary,
            parse_octal,
        )))(number)
        .finish()
        .map_err(|e| {
            serde::de::Error::custom(format!("Could not parse string {s} as integer: {e}"))
        })?;

        Some(s)
    } else {
        Some(s.as_u64().unwrap())
    })
}

fn deserialize_rdl_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    let s: serde_json::Value = Deserialize::deserialize(deserializer)?;

    Ok(if s.is_object() {
        let number = s.get("int").unwrap().as_str().unwrap();

        // Then use nom to parse the string as an integer, this parser accepts (and ignores) '_' in the
        // value.
        let (_, s) = all_consuming(alt((
            parse_hexidecimal,
            parse_decimal,
            parse_binary,
            parse_octal,
        )))(number)
        .finish()
        .map_err(|e| {
            serde::de::Error::custom(format!("Could not parse string {s} as integer: {e}"))
        })?;

        s
    } else {
        s.as_u64().unwrap()
    })
}

fn deserialize_rdl_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s: serde_json::Value = Deserialize::deserialize(deserializer)?;

    Ok(if s.is_object() {
        let mut output = s
            .get("text")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                let mut output = s.as_str().unwrap().to_string();
                output.push('\n');
                output
            })
            .collect::<String>();
        output.pop();
        output
    } else {
        s.as_str().unwrap().to_string()
    })
}

pub enum RdlField {
    MemoryRef {
        mask: u32,
        upper_bits: u32,
        lower_bits: u32,
        description: String,
    },
    Field(RdlDef),
}

impl<'de> Deserialize<'de> for RdlField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DeserializeField;

        impl<'de> Visitor<'de> for DeserializeField {
            type Value = RdlField;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a list either either 4 or 5 elements long")
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let value: (u32, u32, u32, String) =
                    Deserialize::deserialize(SeqAccessDeserializer::new(seq))?;

                Ok(RdlField::MemoryRef {
                    mask: value.0,
                    upper_bits: value.1,
                    lower_bits: value.2,
                    description: value.3,
                })
            }

            fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let output =
                    RdlDef::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(RdlField::Field(output))
            }
        }

        deserializer.deserialize_any(DeserializeField)
    }
}

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct RdlDef {
    #[serde(rename = "Address", deserialize_with = "deserialize_rdl_number")]
    address: u64,
    #[serde(
        rename = "Description",
        deserialize_with = "deserialize_rdl_string",
        default
    )]
    description: String,
    #[serde(
        rename = "ArraySize",
        deserialize_with = "maybe_deserialize_rdl_number",
        default
    )]
    array_size: Option<u64>,
    #[serde(
        rename = "AddressIncrement",
        deserialize_with = "maybe_deserialize_rdl_number",
        default
    )]
    address_increment: Option<u64>,
    #[serde(rename = "Fields", default)]
    fields: HashMap<String, RdlField>,
}

fn parse_json_translation_file(
    path: &str,
    file: &str,
) -> Result<HashMap<String, MemorySlice>, Box<dyn std::error::Error>> {
    fn parse_field(name: &str, field: &RdlField) -> MemorySlice {
        match field {
            RdlField::MemoryRef {
                mask: _,
                upper_bits,
                lower_bits,
                description: _,
            } => {
                let size = ((upper_bits + 1 + 7) / 8) as u64;
                MemorySlice {
                    name: name.to_string(),
                    offset: 0,
                    array_count: None,
                    size,
                    bit_mask: Some((*lower_bits, *upper_bits)),
                    children: HashMap::new(),
                }
            }
            RdlField::Field(field) => convert_to_memory_slice(name, field),
        }
    }

    fn convert_to_memory_slice(name: &str, def: &RdlDef) -> MemorySlice {
        let mut output = MemorySlice {
            name: name.to_string(),
            offset: def.address,
            size: 0,
            array_count: def.array_size,
            bit_mask: None,
            children: HashMap::with_capacity(def.fields.len()),
        };
        for (region_name, def) in &def.fields {
            output
                .children
                .insert(region_name.clone(), parse_field(region_name.as_str(), def));
        }

        output.size = if let Some(incr) = &def.address_increment {
            *incr
        } else {
            output.children.values().map(|v| v.size).sum()
        };

        output
    }

    let top_level: HashMap<String, RdlDef> =
        serde_json::from_slice(&std::fs::read(format!("{path}/{file}"))?)?;

    let mut slices = HashMap::with_capacity(top_level.len());
    for (name, top) in top_level {
        println!("Parsing {name}");

        slices.insert(name.clone(), convert_to_memory_slice(&name, &top));
    }

    Ok(slices)
}

fn parse_json_and_serialize_translation_singlelayer(
    path: &str,
    file: &str,
    output: &str,
    whitelist: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let data = MemorySlices::Tree(parse_json_translation_file(path, file)?);

    let mut output_data = HashMap::new();
    for w in whitelist {
        output_data.insert(w.to_string(), axi_translate(Some(&data), w)?);
    }

    let data = bincode::serialize(&MemorySlices::Flat(output_data))?;
    std::fs::write(output, data)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    /*
        parse_yaml_and_serialize_translation(
            "data/grayskull",
            "axi-pci.yaml",
            "grayskull-axi-pci.bin",
        )?;
        parse_yaml_and_serialize_translation("data/wormhole", "axi-pci.yaml", "wormhole-axi-pci.bin")?;
        parse_yaml_and_serialize_translation("data/wormhole", "axi-noc.yaml", "wormhole-axi-noc.bin")?;
    */

    let os_keys = [
        "ARC_CSM.ARC_PCIE_DMA_REQUEST",
        "ARC_RESET.ARC_MISC_CNTL",
        "ARC_RESET.SCRATCH[0]",
        "ARC_RESET.SCRATCH[1]",
        "ARC_RESET.SCRATCH[2]",
        "ARC_RESET.SCRATCH[3]",
        "ARC_RESET.SCRATCH[4]",
        "ARC_RESET.SCRATCH[5]",
        "ARC_RESET.POST_CODE",
        "ARC_CSM.DATA[0]",
        "ARC_CSM.ARC_PCIE_DMA_REQUEST.trigger",
        "ARC_RESET.GPIO2_PAD_TRIEN_CNTL",
        "ARC_RESET.GPIO2_PAD_DRV_CNTL",
        "ARC_RESET.GPIO2_PAD_RXEN_CNTL",
        "ARC_RESET.SPI_CNTL",
        "ARC_SPI.SPI_CTRLR0",
        "ARC_SPI.SPI_CTRLR1",
        "ARC_SPI.SPI_SSIENR",
        "ARC_SPI.SPI_SER",
        "ARC_SPI.SPI_SR",
        "ARC_SPI.SPI_DR",
        "ARC_SPI.SPI_BAUDR",
    ];
    parse_yaml_and_serialize_translation_singlelayer(
        "data/grayskull",
        "axi-pci.yaml",
        "axi-data/grayskull-axi-pci.bin",
        &os_keys,
    )?;
    let os_keys = [
        [
            "ARC_RESET.REFCLK_COUNTER_LOW",
            "ARC_RESET.REFCLK_COUNTER_HIGH",
        ]
        .as_slice(),
        os_keys.as_slice(),
    ]
    .concat();
    parse_yaml_and_serialize_translation_singlelayer(
        "data/wormhole",
        "axi-noc.yaml",
        "axi-data/wormhole-axi-noc.bin",
        &os_keys,
    )?;
    parse_yaml_and_serialize_translation_singlelayer(
        "data/wormhole",
        "axi-pci.yaml",
        "axi-data/wormhole-axi-pci.bin",
        &os_keys,
    )?;

    let os_keys = [
        "arc_ss.reset_unit.SCRATCH_0",
        "arc_ss.reset_unit.ARC_MISC_CNTL",
        "arc_ss.reset_unit.ARC_MISC_CNTL.irq0_trig",
        "arc_ss.reset_unit.SCRATCH_RAM[0]",
        "arc_ss.reset_unit.SCRATCH_RAM[10]",
        "arc_ss.reset_unit.SCRATCH_RAM[11]",
        "arc_ss.reset_unit.SCRATCH_RAM[12]",
        "arc_ss.reset_unit.SCRATCH_RAM[13]",
    ];
    parse_json_and_serialize_translation_singlelayer(
        "data/blackhole/a0",
        "arc.json",
        "axi-data/blackhole-axi-pci.bin",
        &os_keys,
    )?;

    Ok(())
}
