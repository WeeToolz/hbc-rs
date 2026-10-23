# Reborn storage-load comparison scope

Reference: `/Volumes/SSD/WiiDev/C-sources/hbc-reborn/channel/channelapp`.
Compare SD FAT and USB mass-storage discovery, exact executable reads, DOL
validation, metadata arguments, IOS/AHB policy, cleanup, and application entry.
The normal SD browser path and actual child startup are tested in Dolphin.
Physical Wii SD and USB port behavior require real-device probes and remain
unverified by emulator results. Reborn assigns usb/usb2 in discovery order;
they are drive slots, not fixed physical port labels. Rust enumerates devices
without a physical-port filter but currently exposes only one USB volume.
Concurrent Reborn-style usb/usb2 mounting is a recorded implementation gap,
not a completed feature or an excluded test.
