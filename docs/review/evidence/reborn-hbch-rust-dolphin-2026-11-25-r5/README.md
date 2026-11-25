# Reborn HBCH Rust Dolphin check

The isolated Dolphin check passed for Rust DOL SHA-256
`a09957fdfc9635f069d3b349fbdeee145ab150fb876804b3a082024cd5cc15d4`. The
machine result is in [result.json](result.json). The test checked the zero mask
(all five pages), each page bit by itself, a combined Video and Network mask,
and an unknown-only mask. It parsed each reply as JSON and checked page
selection and the System IOS field.

System reports the console model, serial, region, IOS and AHBPROT, console ID,
Wii Menu, boot2, CPU, Hollywood revision, and language. Video reports all
Reborn fields. Storage reports app mount state and NAND statistics. USB reports
IOS device enumeration and known device descriptions. Network reports assigned
IP and MAC.

This check proves the Rust protocol and current Dolphin API results. It does not
prove full Reborn parity: mounted-volume capacity, GameCube card sizes, and
Network netmask, gateway, DNS, and connection settings remain open. A matched
C/Rust System-page comparison and real Wii device values also remain open. The
local C channel DOL did not start its listener in the same Dolphin check, so it
supplied no comparison result.

The raw Dolphin process log is preserved in [process.log](process.log).
