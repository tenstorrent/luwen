from pyluwen import detect_chips
from pprint import pprint

def main():
    devices = detect_chips()
    for i, dev in enumerate(devices):
        if dev.as_bh():
            flshinfo = dev.as_bh().decode_boot_fs_table("flshinfo")
            print("flshinfo decoded:")
            pprint(flshinfo)

            cmfwcfg = dev.as_bh().decode_boot_fs_table("cmfwcfg")
            print("cmfwcfg decoded:")
            pprint(cmfwcfg)

            cmfwcfg["chip_limits"]["asic_fmax"] = 900
            dev.as_bh().encode_and_write_boot_fs_table(cmfwcfg, "cmfwcfg")

        else:
            print("Nothing to do for non BH chips")

if __name__ == "__main__":
    main()
