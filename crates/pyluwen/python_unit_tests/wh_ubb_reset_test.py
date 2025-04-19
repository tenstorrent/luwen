import time
from pyluwen import detect_chips, run_wh_ubb_ipmi_reset, run_ubb_wait_for_driver_load

def main():
    # Detect chips
    devices = detect_chips()
    for i, dev in enumerate(devices):
        if dev.as_wh():
            # arc_msg to set it into the A3 state
            dev.as_wh().arc_msg(0xA3, wait_for_done=False)
            print("arc_msg sent to WH chip")
    time.sleep(5)
    ubb_num = "0xF"
    dev_num = "0xFF"
    op_mode = "0x0"
    reset_time = "0xF"
    try:
        run_wh_ubb_ipmi_reset(ubb_num, dev_num, op_mode, reset_time)
        time.sleep(30)
        run_ubb_wait_for_driver_load()
        print("ubb reset done")
    except Exception as e:
        print(f"Error: {e}")
        return

    devices = detect_chips()
    print("Num devices detected after reset: ", len(devices))

if __name__ == "__main__":
    main()
