use std::{env, fs};

fn try_to_compiled_proto_file_by_name(name: &str, out_dir: &str) -> Result<(), std::io::Error> {
    let proto_file = format!("{name}.proto");
    let outname = format!("{out_dir}/{name}.rs");
    let mut protoc_build_config = prost_build::Config::new();
    protoc_build_config.out_dir(out_dir);

    // Add `#[derive(Serialize)]` to all generated messages for easy HashMap conversion
    protoc_build_config.type_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");

    protoc_build_config.compile_protos(&[proto_file], &["bh_spirom_protobufs/"])?;
    fs::rename(format!("{out_dir}/_.rs"), outname)?;
    Ok(())
}

fn compiled_proto_file_by_name(
    name: &str,
    out_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let first_try = try_to_compiled_proto_file_by_name(name, out_dir);
    match first_try {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // If we couldn't find the system protoc then use the vendored
            return Err(Box::new(e));
        }
        other => {
            other?;
        }
    }

    Ok(())
}

/// Point prost-build at a protoc that is independent of whatever the system ships.
///
/// fw_table_override.proto uses proto3 `optional`, which needs protoc >= 3.15, and
/// CI environments often ship older versions. On most hosts this is the binary
/// bundled by protoc-bin-vendored; on riscv64, for which no official protoc release
/// binary exists, it is a protoc compiled from source by protobuf-src. An explicitly
/// set PROTOC always wins.
fn configure_protoc() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("PROTOC").is_some() {
        return Ok(());
    }

    #[cfg(target_arch = "riscv64")]
    let protoc = protobuf_src::protoc();
    #[cfg(not(target_arch = "riscv64"))]
    let protoc = protoc_bin_vendored::protoc_bin_path()?;

    env::set_var("PROTOC", protoc);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_protoc()?;

    // Get the output directory from Cargo
    let out_dir = env::var("OUT_DIR")?;
    compiled_proto_file_by_name("fw_table", &out_dir)?;
    compiled_proto_file_by_name("fw_table_override", &out_dir)?;
    compiled_proto_file_by_name("flash_info", &out_dir)?;
    compiled_proto_file_by_name("read_only", &out_dir)?;

    Ok(())
}
