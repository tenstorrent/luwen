from pyluwen import detect_chips

def raw_to_temperature(val):
    raw = (val & 0xFFFF) 
    eqbs = raw / 4096.0 - 0.5
    return 83.09 + 262.5 * eqbs

def main():
    chip = detect_chips()[0]
         # Test ReadTs with sensor index 0
    result = chip.arc_msg(
        msg=0x1B,           # ReadTs message code (from your implementation)
        arg0=0,             # sensor_idx (lower 16 bits)
        arg1=0,             # sensor_idx (upper 16 bits) - 0 for indices < 65536
        wait_for_done=True  # Wait for response
    )

    if result:
        temp_value, return_code = result
        temp_celsius = raw_to_temperature(temp_value)
        print(f"Sensor 0 temperature: {temp_value} (raw) = {temp_celsius:.2f}°C, return code: {return_code}")
    
    # Test different sensor indices (adjust range based on your hardware)
    for sensor_idx in range(8):
        result = chip.arc_msg(
            msg=0x1B,
            arg0=sensor_idx,
            arg1=0,
            wait_for_done=True,
            timeout=2.0  # 2 second timeout
        )

        if result:
            temp_value, return_code = result
            temp_celsius = raw_to_temperature(temp_value)
            print(f"Sensor {sensor_idx}: {temp_value} (raw) = {temp_celsius:.2f}°C, rc={return_code}")
        else:
            print(f"Sensor {sensor_idx}: No response (async call)")

if __name__ == "__main__":
    main()
