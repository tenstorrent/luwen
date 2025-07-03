# Generated Protobuf Code

This directory contains automatically generated Rust code from protobuf definitions.

## Files

- `fw_table.rs` - Generated from `bh_spirom_protobufs/fw_table.proto`
- `flash_info.rs` - Generated from `bh_spirom_protobufs/flash_info.proto`
- `read_only.rs` - Generated from `bh_spirom_protobufs/read_only.proto`

## Why these files are checked in

These files are checked in to avoid dependency on the vendored rust-protoc, which has warnings when building. Instead of requiring protoc to be available during build, the generated code is pre-computed and committed to the repository.

## How to regenerate

If any of the `.proto` files in `bh_spirom_protobufs/` are updated, you'll need to regenerate these files:

### Prerequisites

1. Install protoc: `sudo apt-get install protobuf-compiler`
2. Ensure you have the following dependencies available:
   - `prost-build = "0.13.5"`
   - `serde = {version = "1.0", features = ["derive"]}`

### Method 1: Using the provided script

```bash
cd crates/luwen-if
cargo run --bin generate_protobuf
```

### Method 2: Manual generation

Create a temporary Rust file with the following content:

```rust
use prost_build::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = "src/generated";
    std::fs::create_dir_all(out_dir)?;

    let mut config = Config::new();
    config.out_dir(out_dir);

    // Add serde serialization to all generated messages
    config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    // Generate each protobuf file
    for proto_name in ["fw_table", "flash_info", "read_only"] {
        let proto_file = format!("{}.proto", proto_name);
        let outname = format!("{}/{}.rs", out_dir, proto_name);

        config.compile_protos(&[proto_file], &["bh_spirom_protobufs/"])?;

        // prost-build generates files as _.rs, rename them
        std::fs::rename(format!("{}/_.rs", out_dir), outname)?;
    }

    Ok(())
}
```

Then run:
```bash
cargo run --manifest-path /path/to/temporary/Cargo.toml
```

### Method 3: Using protoc directly

If you prefer to use protoc directly (not recommended as it won't include serde derives):

```bash
cd crates/luwen-if
protoc --rust_out=src/generated bh_spirom_protobufs/fw_table.proto
protoc --rust_out=src/generated bh_spirom_protobufs/flash_info.proto
protoc --rust_out=src/generated bh_spirom_protobufs/read_only.proto
```

## Important Notes

- **Always regenerate all files together** to ensure consistency
- **Update the "Generated on" date** in each file's header after regeneration
- **Test the build** after regeneration to ensure everything still works
- **The generated files are not meant to be edited manually** - any changes will be lost

## Version Information

- Generated using protoc version: 3.21.12
- Generated using prost-build version: 0.13.5
- Last regenerated on: 2025-07-03
