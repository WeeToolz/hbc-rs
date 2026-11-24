# Message Board playtime safety review

Reborn's `playtime_destroy` and `playtime_leave` are one behavior contract. On
startup, C clears the Wii Menu `play_rec.dat` record and uses its timestamp to
identify a Menu launch. On exit, C writes an HBC session to `cdb.vff`; when an
app was launched, it also stores pending app state in HBC title data. On the
next HBC start, it completes that app session if HBC was relaunched directly.

The C logger writes the Wii Menu-owned `cdb.vff` through IOS. It temporarily
patches an IOS NAND permission check in memory under AHBPROT, writes with raw
IOS calls, then restores the patch. This is a hardware-specific privilege
bypass and a persistent system-file write. Rust has no equivalent session
logger or supported API that provides this access.

Rust previously called the checked play-record reset helper at startup without
writing replacement HBC/app sessions. This could erase a Menu record without
recording the sessions that replace it in Reborn. Production startup now keeps
`play_rec.dat` unchanged. The standalone reset helper remains available for its
private-NAND API test; production does not call it.

This is a deliberate safety difference. Do not port the IOS permission patch
or write to live `cdb.vff` without a supported interface and a verified recovery
path. The visible difference is that the Wii Message Board has no HBC/app
session entries from Rust. Do not claim playtime logging parity until that
contract is implemented through a safe write path and verified without risking
user NAND data.

The review used local Reborn source and Rust callers. It changed no NAND data.
Physical Wii tests remain pending.
