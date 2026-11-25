# Reborn HBCH Rust Dolphin check

The isolated Dolphin check passed for Rust DOL SHA-256
`cab969db6ce9777be817ba9e73e2f380fa1fcf6221ca809d8a726a02715eb867`. The
machine result is in [result.json](result.json). It checked the zero mask (all
five pages), each page bit by itself, a combined Video and Network mask, and an
unknown-only mask. It parsed each reply as JSON and checked page selection and
the System IOS field.

The Network page reports IP and netmask, the IOS-reported default gateway and
DNS servers, MAC, and the selected Wii Menu connection type, SSID, security,
DHCP, and proxy settings. In this Dolphin NAND, these values included
`192.168.1.52 / 255.255.255.0`, gateway `192.168.1.255`, DNS
`192.168.1.1, 8.8.4.4`, and Wi-Fi network `Mac` using WPA2 AES. The gateway is
reported from the same IOS route-table fields Reborn reads; it was not
independently compared with the C channel because that DOL did not start its
listener.

The IOS request uses Reborn's `0x1c` ioctlv layout. A regression test checks
the big-endian bytes for the `0xfffe` selector and option value. Other remaining
HBCH fields are mounted-volume capacity and GameCube card sizes. Wii U mode
behavior, a matched C/Rust System-page comparison, and real Wii device values
remain open.

The raw Dolphin process log is preserved in [process.log](process.log).
