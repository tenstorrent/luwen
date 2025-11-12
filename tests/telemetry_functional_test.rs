#![cfg(test)]

use serial_test::serial;

use luwen::api::ChipImpl;

/// Functional tests for chip telemetry
///
/// These tests verify that telemetry is working correctly by:
/// - Checking that telemetry heartbeat values change over time (ARC is running)
/// - Verifying that voltage core readings are within expected ranges
/// - Verifying that TDC (temperature) readings are within expected ranges
///
/// Note: These tests require physical hardware to run. By default, they are
/// annotated with #[ignore] to avoid false failures on systems without hardware.
/// To run all hardware tests:
///
///   cargo test --test telemetry_functional_test -- --ignored
///
/// The tests will automatically detect if compatible hardware is present;
/// if hardware is not found, the test will be skipped.
mod test_utils;

#[serial]
mod tests {
    use std::sync::Mutex;

    use super::*;

    // A HACK to solve problems with resource contention on the ARC in WH
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    #[cfg_attr(not(feature = "test_hardware"), ignore = "Requires real hardware")]
    fn test_telemetry_heartbeat() {
        let _lock = TEST_LOCK.lock();

        let chip = luwen::pci::open(0).unwrap();
        println!("Detected chip; gathering telemetry");

        dbg!(chip.get_arch());

        // Get initial telemetry
        let telem_a = chip.get_telemetry().unwrap();
        println!("Initial heartbeat value: {}", telem_a.telemetry_heartbeat());

        // Sleep to allow time for heartbeat to change
        println!("Sleeping for 1 second before checking telemetry again");
        std::thread::sleep(std::time::Duration::from_secs(1));

        println!("Gathering telemetry again");
        let telem_b = chip.get_telemetry().unwrap();
        println!("New heartbeat value: {}", telem_b.telemetry_heartbeat());

        drop(_lock);

        // Verify the heartbeat changed, indicating ARC is running
        assert_ne!(
            telem_a.telemetry_heartbeat(),
            telem_b.telemetry_heartbeat(),
            "ARC appears to be hung - heartbeat not changing"
        );
    }

    #[test]
    #[cfg_attr(not(feature = "test_hardware"), ignore = "Requires real hardware")]
    fn test_voltage_readings() {
        let _lock = TEST_LOCK.lock();

        let chip = luwen::pci::open(0).unwrap();
        let telemetry = chip.get_telemetry().unwrap();

        drop(_lock);

        // Check vcore is within expected range (700-850 mV)
        println!("Vcore reading: {} mV", telemetry.vcore);
        assert!(
            telemetry.vcore >= 700 && telemetry.vcore <= 850,
            "Board vcore reading is outside of the expected range: {} mV",
            telemetry.vcore
        );
    }

    #[test]
    #[cfg_attr(not(feature = "test_hardware"), ignore = "Requires real hardware")]
    fn test_temperature_readings() {
        let _lock = TEST_LOCK.lock();

        let chip = luwen::pci::open(0).unwrap();
        let telemetry = chip.get_telemetry().unwrap();

        drop(_lock);

        // Check TDC is within expected range (3-200)
        let tdc = telemetry.tdc & 0xFFFF;
        println!("TDC reading: {tdc}");
        assert!(
            (3..=300).contains(&tdc),
            "Board TDC (temperature) reading is outside of the expected range: {tdc}"
        );

        // Check asic temperature
        println!("ASIC temperature: {}", telemetry.asic_temperature());
        assert!(
            telemetry.asic_temperature() > 0.0 && telemetry.asic_temperature() <= 125.0,
            "ASIC temperature reading is outside of expected range: {}",
            telemetry.asic_temperature()
        );
    }

    #[test]
    #[cfg_attr(not(feature = "test_hardware"), ignore = "Requires real hardware")]
    fn test_telemetry_consistency() {
        let _lock = TEST_LOCK.lock();
        let chip = luwen::pci::open(0).unwrap();

        // Take multiple telemetry readings in quick succession
        let telem1 = chip.get_telemetry().unwrap();
        let telem2 = chip.get_telemetry().unwrap();
        let telem3 = chip.get_telemetry().unwrap();

        drop(_lock);

        // Check that board ID and asic ID remain consistent
        assert_eq!(
            telem1.board_id, telem2.board_id,
            "Board ID inconsistent between readings"
        );
        assert_eq!(
            telem2.board_id, telem3.board_id,
            "Board ID inconsistent between readings"
        );

        // Check that asic ID remains consistent
        assert_eq!(
            telem1.asic_id, telem2.asic_id,
            "ASIC ID inconsistent between readings"
        );
        assert_eq!(
            telem2.asic_id, telem3.asic_id,
            "ASIC ID inconsistent between readings"
        );

        // Verify presence of telemetry data
        assert_ne!(telem1.board_id, 0, "Board ID should not be zero");

        println!("Board ID: {:X}", telem1.board_id);
        println!("ASIC ID: {:X}", telem1.asic_id);
    }
}
