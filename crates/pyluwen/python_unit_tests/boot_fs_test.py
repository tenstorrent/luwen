from pyluwen import detect_chips
from pprint import pprint

def main():
    devices = detect_chips()
    for i, dev in enumerate(devices):
        if dev.as_bh():
            boardcfg_addr = dev.as_bh().get_spirom_table_spi_addr("boardcfg")
            boardcfg_size = dev.as_bh().get_spirom_table_image_size("boardcfg")
            print("boardcfg addr:", hex(boardcfg_addr), "size:", boardcfg_size)
            
            flshinfo_addr = dev.as_bh().get_spirom_table_spi_addr("flshinfo")
            flshinfo_size = dev.as_bh().get_spirom_table_image_size("flshinfo")
            print("flshinfo addr:", hex(flshinfo_addr), "size:", flshinfo_size)
            
            cmfwcfg_addr = dev.as_bh().get_spirom_table_spi_addr("cmfwcfg")
            cmfwcfg_size = dev.as_bh().get_spirom_table_image_size("cmfwcfg")
            print("cmfwcfg addr:", hex(cmfwcfg_addr), "size:", cmfwcfg_size)
            
            origcfg_addr = dev.as_bh().get_spirom_table_spi_addr("origcfg")
            origcfg_size = dev.as_bh().get_spirom_table_image_size("origcfg")
            print("origcfg addr:", hex(origcfg_addr), "size:", origcfg_size)
            
        else:
            print("Nothing to do for non BH chips")

if __name__ == "__main__":
    main()
