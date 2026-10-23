//! Emulator acceptance entry. This module is absent from normal HBC builds.
use crate::{loader, metadata::UserSettings, platform::settings};
use alloc::string::String;

fn expected_settings() -> UserSettings {
    UserSettings {
        device: 1,
        sort_order: 2,
        browse_mode: 1,
        selected_app: Some(String::from("sd:/apps/acceptance/boot.dol")),
    }
}

/// # Safety
/// The runtime must supply a valid argc/argv vector. The caller must establish
/// that this process uses an isolated emulator NAND before it enables entry.
pub unsafe fn run(argc: i32, argv: *mut *mut u8) -> i32 {
    if argc != 2 || argv.is_null() {
        return -22;
    }
    let pointer = unsafe { *argv.add(1) };
    if pointer.is_null() {
        return -22;
    }
    let case = unsafe { core::ffi::CStr::from_ptr(pointer.cast()) }.to_bytes();
    let result = match case {
        b"settings-save" => settings::save(&expected_settings()),
        b"settings-load" => settings::load().and_then(|actual| {
            if actual == expected_settings() {
                Ok(())
            } else {
                Err(settings::SettingsIoError::Codec)
            }
        }),
        b"settings-banner-error" => expect_banner_error(),
        b"settings-banner-retry" => {
            expect_banner_error().and_then(|()| settings::save(&expected_settings()))
        }
        b"settings-direct" => settings::persistence_available().and_then(|available| {
            if !available {
                Ok(())
            } else {
                Err(settings::SettingsIoError::InvalidPath)
            }
        }),
        b"launch-dol" | b"launch-elf" => return launch(case == b"launch-elf"),
        _ => return -22,
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            ogc_system::report(format_args!("FAIL acceptance {error:?}\n"));
            -5
        }
    }
}

fn expect_banner_error() -> Result<(), settings::SettingsIoError> {
    match settings::save(&expected_settings()) {
        Err(settings::SettingsIoError::Isfs(-1022)) => Ok(()),
        Err(error) => Err(error),
        Ok(()) => Err(settings::SettingsIoError::Codec),
    }
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn launch(elf: bool) -> i32 {
    unsafe extern "C" {
        static hbc_acceptance_child_start: u8;
        static hbc_acceptance_child_end: u8;
    }
    let start = core::ptr::addr_of!(hbc_acceptance_child_start);
    let length = core::ptr::addr_of!(hbc_acceptance_child_end) as usize - start as usize;
    if length != 0x100 {
        return -22;
    }
    // The linked child is position independent and runs only after the actual
    // production trampoline has copied its image and initialized its memory.
    let payload = unsafe { core::slice::from_raw_parts(start, length) };
    let mut bytes = alloc::vec![0; 0x204];
    if elf {
        bytes[..7].copy_from_slice(b"\x7fELF\x01\x02\x01");
        bytes[16..20].copy_from_slice(&[0, 2, 0, 20]);
        put32(&mut bytes, 20, 1);
        put32(&mut bytes, 24, 0x80010000);
        put32(&mut bytes, 28, 52);
        bytes[40..46].copy_from_slice(&[0, 52, 0, 32, 0, 3]);
        for (index, (offset, address, size, memory, flags)) in [
            (0x100, 0x80010000, 0x100, 0x100, 5),
            (0x200, 0x80012000, 4, 8, 6),
            (0x204, 0x80013000, 0, 4, 6),
        ]
        .iter()
        .enumerate()
        {
            let base = 52 + index * 32;
            for (field, value) in [1, *offset, *address, *address, *size, *memory, *flags, 4]
                .iter()
                .enumerate()
            {
                put32(&mut bytes, base + field * 4, *value);
            }
        }
    } else {
        put32(&mut bytes, 0, 0x100);
        put32(&mut bytes, 0x48, 0x80010000);
        put32(&mut bytes, 0x90, 0x100);
        put32(&mut bytes, 0x1c, 0x200);
        put32(&mut bytes, 0x64, 0x80012000);
        put32(&mut bytes, 0xac, 4);
        put32(&mut bytes, 0xd8, 0x80012004);
        put32(&mut bytes, 0xdc, 0x1000);
        put32(&mut bytes, 0xe0, 0x80010000);
    }
    bytes[0x100..0x200].copy_from_slice(payload);
    put32(&mut bytes, 0x200, 0x12345678);
    let plan = if elf {
        loader::parse_elf32_ppc(&bytes, None)
    } else {
        loader::parse_dol(&bytes, None)
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(_) => return -22,
    };
    let args = match loader::build_launch_arguments("boot.dol", &[]) {
        Ok(args) => args,
        Err(_) => return -22,
    };
    let image = match loader::stage_launch(
        loader::LaunchSource::from_vec(bytes),
        &plan,
        args,
        0x80003400..0x8132fc00,
    ) {
        Ok(image) => image,
        Err(error) => {
            ogc_system::report(format_args!("FAIL stage {error:?}\n"));
            return -5;
        }
    };
    // Poison both zero-fill destinations. A missing copy/clear must fail.
    unsafe {
        core::ptr::write_volatile(0x80012004 as *mut u32, 0xa5a5a5a5);
        core::ptr::write_volatile(0x80013000 as *mut u32, 0xa5a5a5a5);
        loader::transfer_control(
            image,
            loader::IosLaunchPolicy {
                reload: false,
                ahb_access: false,
            },
        )
    }
}

core::arch::global_asm!(include_str!("acceptance_child.S"));
