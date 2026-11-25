# Network scan and USB follow-up

The user reported that a FAT32 USB drive was not detected on a physical Wii.
The report identifies USB 1; the exact mount or enumeration error is not yet
known. HBC's USB 1 and USB 2 choices are discovery-order volumes, not fixed
physical ports. The physical report remains open. A Dolphin SD fixture does
not establish physical USB enumeration or FAT mounting.

The 2026-11-24 Dolphin network scan reached the wireless driver and returned
one record. HBC rejected it: its declared length was 60 bytes, while the local
libogc `BSSDescriptor` includes 62 bytes of fixed fields. The diagnostic
records numeric lengths only, without recording the SSID or access point address.
The [test result](evidence/network-scan-trace-dolphin-2026-11-24/result.json)
and [raw log](evidence/network-scan-trace-dolphin-2026-11-24/dolphin.log)
show the rejection and receiver restart. The UI test passes its error-path
checks; the network scan did not succeed.

The current [Dolphin source](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/IOS/Network/WD/Command.cpp)
generates a synthetic access point. Its [record declaration](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/IOS/Network/WD/Command.h)
declares a nested rate type without a rate field. This matches the 60-byte
record observed in this run, but the installed Dolphin binary was not built
from a source revision verified here. Do not accept a short physical driver
record on this basis. Real Wii network scan results remain unverified.
