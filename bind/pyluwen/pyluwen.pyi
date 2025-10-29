# Generated content DO NOT EDIT
@staticmethod
def detect_chips():
    """
    """
    pass

class AxiData:
    @property
    def addr(self):
        """
        """
        pass

    @property
    def size(self):
        """
        """
        pass

    pass

class DmaBuffer:
    def get_physical_address(self):
        """
        """
        pass

    def get_user_address(self):
        """
        """
        pass

    pass

class PciChip:
    def __init__(self, pci_interface=None):
        pass

    def arc_msg(self, msg, wait_for_done=True, use_second_mailbox=False, arg0=65535, arg1=65535, timeout=1.0):
        """
        """
        pass

    def as_gs(self):
        """
        """
        pass

    def as_wh(self):
        """
        """
        pass

    def axi_read(self, addr, data):
        """
        """
        pass

    def axi_read32(self, addr):
        """
        """
        pass

    def axi_translate(self, addr):
        """
        """
        pass

    def axi_write(self, addr, data):
        """
        """
        pass

    def axi_write32(self, addr, data):
        """
        """
        pass

    def bar_size(self):
        """
        """
        pass

    def board_id(self):
        """
        """
        pass

    def device_id(self):
        """
        """
        pass

    def get_pci_bdf(self):
        """
        """
        pass

    def get_telemetry(self):
        """
        """
        pass

    def init(self):
        """
        """
        pass

    def noc_broadcast(self, noc_id, addr, data):
        """
        """
        pass

    def noc_broadcast32(self, noc_id, addr, data):
        """
        """
        pass

    def noc_read(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_read32(self, noc_id, x, y, addr):
        """
        """
        pass

    def noc_write(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_write32(self, noc_id, x, y, addr, data):
        """
        """
        pass

    pass

class PciGrayskull:
    def arc_msg(self, msg, wait_for_done=True, use_second_mailbox=False, arg0=65535, arg1=65535, timeout=1.0):
        """
        """
        pass

    def axi_read(self, addr, data):
        """
        """
        pass

    def axi_read32(self, addr):
        """
        """
        pass

    def axi_translate(self, addr):
        """
        """
        pass

    def axi_write(self, addr, data):
        """
        """
        pass

    def axi_write32(self, addr, data):
        """
        """
        pass

    def noc_broadcast(self, noc_id, addr, data):
        """
        """
        pass

    def noc_broadcast32(self, noc_id, addr, data):
        """
        """
        pass

    def noc_read(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_read32(self, noc_id, x, y, addr):
        """
        """
        pass

    def noc_write(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_write32(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def pci_axi_read32(self, addr):
        """
        """
        pass

    def pci_axi_write32(self, addr, data):
        """
        """
        pass

    def pci_board_type(self):
        """
        """
        pass

    def set_default_tlb(self, index):
        """
        """
        pass

    def setup_tlb(self, index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked):
        """
        """
        pass

    pass

class PciWormhole:
    def allocate_dma_buffer(self, size):
        """
        """
        pass

    def arc_msg(self, msg, wait_for_done=True, use_second_mailbox=False, arg0=65535, arg1=65535, timeout=1.0):
        """
        """
        pass

    def axi_read(self, addr, data):
        """
        """
        pass

    def axi_read32(self, addr):
        """
        """
        pass

    def axi_translate(self, addr):
        """
        """
        pass

    def axi_write(self, addr, data):
        """
        """
        pass

    def axi_write32(self, addr, data):
        """
        """
        pass

    def config_dma(self, dma_64_bit_addr, csm_pcie_ctrl_dma_request_offset, arc_misc_cntl_addr, msi, read_threshold, write_threshold):
        """
        """
        pass

    def dma_transfer_turbo(self, addr, physical_dma_buffer, size, write):
        """
        """
        pass

    def noc_broadcast(self, noc_id, addr, data):
        """
        """
        pass

    def noc_broadcast32(self, noc_id, addr, data):
        """
        """
        pass

    def noc_read(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_read32(self, noc_id, x, y, addr):
        """
        """
        pass

    def noc_write(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_write32(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def open_remote(self, rack_x=None, rack_y=None, shelf_x=None, shelf_y=None):
        """
        """
        pass

    def set_default_tlb(self, index):
        """
        """
        pass

    def setup_tlb(self, index, addr, x_start, y_start, x_end, y_end, noc_sel, mcast, ordering, linked):
        """
        """
        pass

    pass

class RemoteWormhole:
    def arc_msg(self, msg, wait_for_done=True, use_second_mailbox=False, arg0=65535, arg1=65535, timeout=1.0):
        """
        """
        pass

    def axi_read(self, addr, data):
        """
        """
        pass

    def axi_read32(self, addr):
        """
        """
        pass

    def axi_translate(self, addr):
        """
        """
        pass

    def axi_write(self, addr, data):
        """
        """
        pass

    def axi_write32(self, addr, data):
        """
        """
        pass

    def noc_broadcast(self, noc_id, addr, data):
        """
        """
        pass

    def noc_broadcast32(self, noc_id, addr, data):
        """
        """
        pass

    def noc_read(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_read32(self, noc_id, x, y, addr):
        """
        """
        pass

    def noc_write(self, noc_id, x, y, addr, data):
        """
        """
        pass

    def noc_write32(self, noc_id, x, y, addr, data):
        """
        """
        pass

    pass
