# HELP process_cpu_seconds_total Total user and system CPU time spent in seconds.
# TYPE process_cpu_seconds_total counter
process_cpu_seconds_total 0
# HELP process_max_fds Maximum number of open file descriptors.
# TYPE process_max_fds gauge
process_max_fds 1048576
# HELP process_open_fds Number of open file descriptors.
# TYPE process_open_fds gauge
process_open_fds 18
# HELP process_resident_memory_bytes Resident memory size in bytes.
# TYPE process_resident_memory_bytes gauge
process_resident_memory_bytes 5984256
# HELP process_start_time_seconds Start time of the process since unix epoch in seconds.
# TYPE process_start_time_seconds gauge
process_start_time_seconds 1702586675
# HELP process_threads Number of OS threads in the process.
# TYPE process_threads gauge
process_threads 8
# HELP process_virtual_memory_bytes Virtual memory size in bytes.
# TYPE process_virtual_memory_bytes gauge
process_virtual_memory_bytes 1071894528
# HELP prometheus_exporter_request_duration_seconds The HTTP request latencies in seconds.
# TYPE prometheus_exporter_request_duration_seconds histogram
prometheus_exporter_request_duration_seconds_bucket{le="0.005"} 2
prometheus_exporter_request_duration_seconds_bucket{le="0.01"} 2
prometheus_exporter_request_duration_seconds_bucket{le="0.025"} 2
prometheus_exporter_request_duration_seconds_bucket{le="0.05"} 2
prometheus_exporter_request_duration_seconds_bucket{le="0.1"} 2
prometheus_exporter_request_duration_seconds_bucket{le="0.25"} 2
prometheus_exporter_request_duration_seconds_bucket{le="0.5"} 2
prometheus_exporter_request_duration_seconds_bucket{le="1"} 2
prometheus_exporter_request_duration_seconds_bucket{le="2.5"} 2
prometheus_exporter_request_duration_seconds_bucket{le="5"} 2
prometheus_exporter_request_duration_seconds_bucket{le="10"} 2
prometheus_exporter_request_duration_seconds_bucket{le="+Inf"} 2
prometheus_exporter_request_duration_seconds_sum 0.00000786
prometheus_exporter_request_duration_seconds_count 2
# HELP prometheus_exporter_requests_total Number of HTTP requests received.
# TYPE prometheus_exporter_requests_total counter
prometheus_exporter_requests_total 2
# HELP prometheus_exporter_response_size_bytes The HTTP response sizes in bytes.
# TYPE prometheus_exporter_response_size_bytes gauge
prometheus_exporter_response_size_bytes 5953
# HELP tt_smi_aiclk AICLK (MHz)
# TYPE tt_smi_aiclk gauge
tt_smi_aiclk{board_id="0100014511708037_pcie"} 500
tt_smi_aiclk{board_id="0100014511708037_remote"} 500
# HELP tt_smi_aixclk AXICLK (MHz)
# TYPE tt_smi_aixclk gauge
tt_smi_aixclk{board_id="0100014511708037_pcie"} 900
tt_smi_aixclk{board_id="0100014511708037_remote"} 900
# HELP tt_smi_arcclk ARCCLK (MHz)
# TYPE tt_smi_arcclk gauge
tt_smi_arcclk{board_id="0100014511708037_pcie"} 540
tt_smi_arcclk{board_id="0100014511708037_remote"} 540
# HELP tt_smi_asic_temperature Core Temp (C)
# TYPE tt_smi_asic_temperature gauge
tt_smi_asic_temperature{board_id="0100014511708037_pcie"} 57
tt_smi_asic_temperature{board_id="0100014511708037_remote"} 38
# HELP tt_smi_board_temperature_0 Outlet Temp 2 (C)
# TYPE tt_smi_board_temperature_0 gauge
tt_smi_board_temperature_0{board_id="0100014511708037_pcie"} 41
tt_smi_board_temperature_0{board_id="0100014511708037_remote"} 41
# HELP tt_smi_board_temperature_1 Outlet Temp 1 (C)
# TYPE tt_smi_board_temperature_1 gauge
tt_smi_board_temperature_1{board_id="0100014511708037_pcie"} 43
tt_smi_board_temperature_1{board_id="0100014511708037_remote"} 43
# HELP tt_smi_board_temperature_2 Inlet Temp (C)
# TYPE tt_smi_board_temperature_2 gauge
tt_smi_board_temperature_2{board_id="0100014511708037_pcie"} 37
tt_smi_board_temperature_2{board_id="0100014511708037_remote"} 37
# HELP tt_smi_cur_pci_link_gen Current PCIe gen
# TYPE tt_smi_cur_pci_link_gen gauge
tt_smi_cur_pci_link_gen{board_id="0100014511708037_pcie"} 4
# HELP tt_smi_cur_pci_link_width Current PCIe width
# TYPE tt_smi_cur_pci_link_width gauge
tt_smi_cur_pci_link_width{board_id="0100014511708037_pcie"} 16
# HELP tt_smi_current Core Current (A)
# TYPE tt_smi_current gauge
tt_smi_current{board_id="0100014511708037_pcie"} 15
tt_smi_current{board_id="0100014511708037_remote"} 14
# HELP tt_smi_max_pci_link_gen Max PCIe gen
# TYPE tt_smi_max_pci_link_gen gauge
tt_smi_max_pci_link_gen{board_id="0100014511708037_pcie"} 4
# HELP tt_smi_max_pci_link_width Max PCIe width
# TYPE tt_smi_max_pci_link_width gauge
tt_smi_max_pci_link_width{board_id="0100014511708037_pcie"} 16
# HELP tt_smi_pci_bus pci.bus
# TYPE tt_smi_pci_bus gauge
tt_smi_pci_bus{board_id="0100014511708037_pcie"} 97
# HELP tt_smi_pci_device pci.device
# TYPE tt_smi_pci_device gauge
tt_smi_pci_device{board_id="0100014511708037_pcie"} 0
# HELP tt_smi_pci_device_id pci.device_id
# TYPE tt_smi_pci_device_id gauge
tt_smi_pci_device_id{board_id="0100014511708037_pcie"} 16414
# HELP tt_smi_pci_function pci.function
# TYPE tt_smi_pci_function gauge
tt_smi_pci_function{board_id="0100014511708037_pcie"} 0
# HELP tt_smi_pci_vendor_id pci.vendor_id
# TYPE tt_smi_pci_vendor_id gauge
tt_smi_pci_vendor_id{board_id="0100014511708037_pcie"} 7762
# HELP tt_smi_power Core Power (W)
# TYPE tt_smi_power gauge
tt_smi_power{board_id="0100014511708037_pcie"} 12
tt_smi_power{board_id="0100014511708037_remote"} 11
# HELP tt_smi_sw_info Always 1; labeled with software versions
# TYPE tt_smi_sw_info gauge
tt_smi_sw_info{arc_fw_ver="16.0.0",board_id="0100014511708037_pcie",board_type="n300",eth_fw_ver="6.3.0",fw_date="2023-08-29"} 1
tt_smi_sw_info{arc_fw_ver="16.0.0",board_id="0100014511708037_remote",board_type="n300",eth_fw_ver="6.3.0",fw_date="2023-08-29"} 1
# HELP tt_smi_tt_interface_id N in /dev/tenstorrent/N
# TYPE tt_smi_tt_interface_id gauge
tt_smi_tt_interface_id{board_id="0100014511708037_pcie"} 7
# HELP tt_smi_voltage Core Voltage (V)
# TYPE tt_smi_voltage gauge
tt_smi_voltage{board_id="0100014511708037_pcie"} 0.72
tt_smi_voltage{board_id="0100014511708037_remote"} 0.72
# HELP tt_smi_vreg_temperature VREG Temp (C)
# TYPE tt_smi_vreg_temperature gauge
tt_smi_vreg_temperature{board_id="0100014511708037_pcie"} 44
tt_smi_vreg_temperature{board_id="0100014511708037_remote"} 34
