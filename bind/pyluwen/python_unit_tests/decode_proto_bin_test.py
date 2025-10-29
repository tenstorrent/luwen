from pyluwen import detect_chips
from pprint import pprint

def main():
    devices = detect_chips()
    for i, dev in enumerate(devices):
        if dev.as_bh():
            boardcfg = dev.as_bh().decode_boot_fs_table("boardcfg")
            print("boardcfg decoded:")
            pprint(boardcfg)

            flshinfo = dev.as_bh().decode_boot_fs_table("flshinfo")
            print("flshinfo decoded:")
            pprint(flshinfo)

            cmfwcfg = dev.as_bh().decode_boot_fs_table("cmfwcfg")
            print("cmfwcfg decoded:")
            pprint(cmfwcfg)

            origcfg = dev.as_bh().decode_boot_fs_table("origcfg")
            print("origcfg decoded:")
            pprint(origcfg)

        else:
            print("Nothing to do for non BH chips")

if __name__ == "__main__":
    main()
