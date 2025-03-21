/*!
File: prometheus-exporter/src/main.rs

Description:
- Collects card health and software version information from WH/GS chips.
- Exposes this data as a Prometheus endpoint.

Details:
- A Prometheus endpoint refers to a URL where Prometheus can scrape metrics.
- A Prometheus exporter is a service that translates metrics from some source
  and exports them as an endpoint.
- You can test this code by running `curl localhost:8080/metrics` while the
  service is running.  Note that you may need to change the port.

Limitations:
- Not tested on Galaxy systems.
- Requires firmware that supports telemetry gathering.
*/

use clap::Parser;
use luwen_if::chip::Telemetry;
use luwen_if::{ChipImpl, DeviceInfo};
use prometheus::{register_gauge_vec, GaugeVec, Opts};
use std::thread;
use std::time::Duration;

/// Command line arguments.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CommandLineArguments {
    /// Milliseconds in between queries to hardware
    #[arg(short, long, default_value_t = 1000)]
    interval: u16,

    /// Port to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Ignore GS cards
    #[arg(short, long, default_value_t = false)]
    no_grayskull: bool,
}

/// Encapsulates prometheus metrics for all boards in a host.
struct Metrics {
    sw_info: GaugeVec,             // FW versions, dates
    aiclk: GaugeVec,               // AI clock, MHz
    axiclk: GaugeVec,              // AXI clock, MHz
    arcclk: GaugeVec,              // ARC clock, MHz
    voltage: GaugeVec,             // Volts
    asic_temperature: GaugeVec,    // Celsius
    vreg_temperature: GaugeVec,    // Celsius
    outlet_temperature2: GaugeVec, // Celsius
    outlet_temperature1: GaugeVec, // Celsius
    inlet_temperature: GaugeVec,   // Celsius
    power: GaugeVec,               // Watts
    current: GaugeVec,             // Amps
    pci_bus: GaugeVec,             // B in Bus, Device, Function (BDF)
    pci_device: GaugeVec,          // D in BDF
    pci_function: GaugeVec,        // F in BDF
    pci_vendor_id: GaugeVec,       // e.g. 0x1e520
    pci_device_id: GaugeVec,       // e.g. 0x401e
    tt_interface_id: GaugeVec,     // N in /dev/tenstorrent/N; 0 <= N < ??
    pci_cur_link_width: GaugeVec,  // Current PCIe link width
    pci_cur_link_gen: GaugeVec,    // Current PCIe link generation
    pci_max_link_width: GaugeVec,  // Maximum PCIe link width
    pci_max_link_gen: GaugeVec,    // Maximum PCIe link generation
}

macro_rules! register_gauge_vec_with_board_id {
    ($name:expr, $desc:expr) => {{
        let opts = Opts::new($name, $desc).namespace("tt").subsystem("smi");
        register_gauge_vec!(opts, &["board_id"]).unwrap()
    }};
}

macro_rules! set_with_board_id {
    ($gauge:expr, $board_id:expr, $value:expr) => {
        $gauge.with_label_values(&[$board_id]).set($value as f64);
    };
}

impl Metrics {
    pub fn new() -> Metrics {
        Metrics {
            sw_info: {
                let opts = Opts::new("sw_info", "Always 1; labeled with software versions")
                    .namespace("tt")
                    .subsystem("smi");
                register_gauge_vec!(
                    opts,
                    &[
                        "board_id",
                        "fw_date",
                        "arc_fw_ver",
                        "eth_fw_ver",
                        "board_type"
                    ]
                )
                .unwrap()

                // NB: "labeled with software versions" means the metric shows up as e.g.
                // tt_smi_sw_info{arc_fw_ver="16.0.0",board_id="010001851170700c",fw_date="2023-08-29",...} 1
            },
            aiclk: register_gauge_vec_with_board_id!("aiclk", "AICLK (MHz)"),
            axiclk: register_gauge_vec_with_board_id!("aixclk", "AXICLK (MHz)"),
            arcclk: register_gauge_vec_with_board_id!("arcclk", "ARCCLK (MHz)"),
            voltage: register_gauge_vec_with_board_id!("voltage", "Core Voltage (V)"),
            asic_temperature: register_gauge_vec_with_board_id!(
                "asic_temperature",
                "Core Temp (C)"
            ),
            vreg_temperature: register_gauge_vec_with_board_id!(
                "vreg_temperature",
                "VREG Temp (C)"
            ),
            outlet_temperature2: register_gauge_vec_with_board_id!(
                "board_temperature_0",
                "Outlet Temp 2 (C)"
            ),
            outlet_temperature1: register_gauge_vec_with_board_id!(
                "board_temperature_1",
                "Outlet Temp 1 (C)"
            ),
            inlet_temperature: register_gauge_vec_with_board_id!(
                "board_temperature_2",
                "Inlet Temp (C)"
            ),
            power: register_gauge_vec_with_board_id!("power", "Core Power (W)"),
            current: register_gauge_vec_with_board_id!("current", "Core Current (A)"),
            pci_bus: register_gauge_vec_with_board_id!("pci_bus", "pci.bus"),
            pci_device: register_gauge_vec_with_board_id!("pci_device", "pci.device"),
            pci_function: register_gauge_vec_with_board_id!("pci_function", "pci.function"),
            pci_vendor_id: register_gauge_vec_with_board_id!("pci_vendor_id", "pci.vendor_id"),
            pci_device_id: register_gauge_vec_with_board_id!("pci_device_id", "pci.device_id"),
            tt_interface_id: register_gauge_vec_with_board_id!(
                "tt_interface_id",
                "N in /dev/tenstorrent/N"
            ),
            pci_cur_link_width: register_gauge_vec_with_board_id!(
                "cur_pci_link_width",
                "Current PCIe width"
            ),
            pci_cur_link_gen: register_gauge_vec_with_board_id!(
                "cur_pci_link_gen",
                "Current PCIe gen"
            ),
            pci_max_link_width: register_gauge_vec_with_board_id!(
                "max_pci_link_width",
                "Max PCIe width"
            ),
            pci_max_link_gen: register_gauge_vec_with_board_id!("max_pci_link_gen", "Max PCIe gen"),
        }
    }

    /// Call with telemetry for any board.
    pub fn update(&self, device_info: &Option<DeviceInfo>, telemetry: &Telemetry) {
        let mut board_id = telemetry.board_serial_number_hex();

        match device_info {
            // This chip is connected via PCIe.
            Some(info) => {
                board_id.push_str("_pcie");

                // PCI BDF (bus/device/function)
                set_with_board_id!(self.pci_bus, &board_id, info.bus);
                set_with_board_id!(self.pci_device, &board_id, info.slot);
                set_with_board_id!(self.pci_function, &board_id, info.function);

                // PCI link info
                set_with_board_id!(
                    self.pci_cur_link_width,
                    &board_id,
                    info.pcie_current_link_width()
                );
                set_with_board_id!(
                    self.pci_cur_link_gen,
                    &board_id,
                    info.pcie_current_link_gen()
                );
                set_with_board_id!(
                    self.pci_max_link_width,
                    &board_id,
                    info.pcie_max_link_width()
                );
                set_with_board_id!(self.pci_max_link_gen, &board_id, info.pcie_max_link_gen());

                // Luwen uses `device_id` to describe N in /dev/tenstorrent/N in
                // some places, interface_id elsewhere.  Here we're concerned
                // with the PCI device & vendor IDs, that is, the 1e52:401e in
                // e.g. 01:00.0 Processing accelerators: Device 1e52:401e
                //                                              ^^^^ ^^^^
                set_with_board_id!(self.pci_device_id, &board_id, info.device_id);
                set_with_board_id!(self.pci_vendor_id, &board_id, info.vendor);

                // The N in /dev/tenstorrent/N
                set_with_board_id!(self.tt_interface_id, &board_id, info.interface_id);
            }
            // Currently, only PCIe-connected chips have a DeviceInfo.
            None => {
                board_id.push_str("_remote");
            }
        }

        let fw_date = telemetry.firmware_date();
        let arc_fw_ver = telemetry.arc_fw_version();
        let eth_fw_ver = telemetry.eth_fw_version();
        let board_type = telemetry.board_type();
        self.sw_info
            .with_label_values(&[&board_id, &fw_date, &arc_fw_ver, &eth_fw_ver, board_type])
            .set(1.0);

        set_with_board_id!(&self.aiclk, &board_id, telemetry.ai_clk());
        set_with_board_id!(&self.axiclk, &board_id, telemetry.axi_clk());
        set_with_board_id!(&self.arcclk, &board_id, telemetry.arc_clk());
        set_with_board_id!(&self.voltage, &board_id, telemetry.voltage());
        set_with_board_id!(
            &self.asic_temperature,
            &board_id,
            telemetry.asic_temperature()
        );
        set_with_board_id!(
            &self.vreg_temperature,
            &board_id,
            telemetry.vreg_temperature()
        );
        set_with_board_id!(
            &self.inlet_temperature,
            &board_id,
            telemetry.inlet_temperature()
        );
        set_with_board_id!(
            &self.outlet_temperature1,
            &board_id,
            telemetry.outlet_temperature1()
        );
        set_with_board_id!(
            &self.outlet_temperature2,
            &board_id,
            telemetry.outlet_temperature2()
        );
        set_with_board_id!(&self.power, &board_id, telemetry.power());
        set_with_board_id!(&self.current, &board_id, telemetry.current());
    }
}

fn main() {
    let args = CommandLineArguments::parse();
    let interval_ms = args.interval;
    let chips = luwen_ref::detect_chips().unwrap();

    let worker = thread::spawn(move || {
        let metrics = Metrics::new();

        loop {
            for chip in &chips {
                if args.no_grayskull && chip.as_wh().is_none() {
                    continue;
                }
                let device_info = chip.get_device_info().unwrap();
                let telemetry = chip.get_telemetry().unwrap();
                metrics.update(&device_info, &telemetry);
            }

            thread::sleep(Duration::from_millis(interval_ms as u64));
        }
    });

    prometheus_exporter::start(format!("0.0.0.0:{}", args.port).parse().unwrap())
        .expect("failed to start prometheus exporter");

    worker.join().unwrap();
}
