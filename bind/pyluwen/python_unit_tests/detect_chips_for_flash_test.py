"""Unit tests for pyluwen.detect_chips_for_flash."""

from pyluwen import detect_chips_for_flash, pci_scan


def test_detect_chips_for_flash_is_exported():
    assert callable(detect_chips_for_flash)


def test_detect_chips_for_flash_returns_list_when_devices_present():
    if not pci_scan():
        return

    chips = detect_chips_for_flash()
    assert isinstance(chips, list)
    for chip in chips:
        assert hasattr(chip, "as_bh")
        assert hasattr(chip, "as_wh")
