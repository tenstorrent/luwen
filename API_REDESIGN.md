The current API works pretty well, but has a few failings

1. BH requires access to chip telemetry to get both the size of the chip for the broadcast as well as the grid size for the translated coords/enabled drams
2. The feature of using luwen-if as a pluggable middle end is never used

Therefore luwen should be refactored to allow tighter integration between the transport and chip interface implementations. In the simplest sense luwen-ref should be removed and luwen-if should be renamed luwen with the base of the repo turning into a virtual workspace definition.

Next the Chip should still be a container for other Chip types, but the specialization should be explicit. For example if I want to write on the noc I should pull out a `Noc` type from the inner chip and explicity write to it. But if I just want to access the ARC for example I pull out an `Arc` type and use that. This will make workaround for things that aren't 100% natural to a specific transport more explicit. For example trying to access `PciDma` via JTAG doesn't make sense and therefore an error will be returned if an attempt is make to access chip.get("PciDma") (this is the API for accessing each module) to get the type safe implementation it is better to perform chip.as_bh().unwrap().pci_dma().

Certain top level functions will always be available however. i.e. chip.get_telemetry("aiclk") again it can be specialized. chip.as_bh().unwrap().get_telemetry()?.aiclk.
