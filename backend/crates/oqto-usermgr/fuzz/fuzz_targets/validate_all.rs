#![no_main]
use libfuzzer_sys::fuzz_target;
use oqto_usermgr::validate::*;

/// Fuzz all validators with the same input to find edge cases
/// in the interaction between validators.
fuzz_target!(|data: &str| {
    // Run all validators -- none should panic
    let _ = validate_username(data);
    let _ = validate_group(data);
    let _ = validate_shell(data);
    let _ = validate_gecos(data);
    let _ = validate_owner(data);
    let _ = validate_chmod_mode(data);
    let _ = validate_path(data, &["/run/oqto/runner-sockets/", "/home/oqto_"]);

    // If it parses as a u32, test UID validation
    if let Ok(uid) = data.parse::<u32>() {
        let _ = validate_uid(uid);
    }
});
