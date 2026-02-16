#![no_main]
use libfuzzer_sys::fuzz_target;
use octo_usermgr::validate::*;

fuzz_target!(|data: &str| {
    let result = validate_gecos(data);

    // Invariant: if validation passes, GECOS must be safe for /etc/passwd
    if result.is_ok() {
        assert!(
            data.starts_with(GECOS_PREFIX),
            "Accepted GECOS without required prefix: {data:?}"
        );
        assert!(
            data.len() <= GECOS_MAX_LEN,
            "Accepted GECOS too long: {data:?}"
        );
        // Must not contain /etc/passwd field separator
        assert!(
            !data.contains(':'),
            "Accepted GECOS with colon (passwd separator): {data:?}"
        );
        // Must not contain newlines (could inject passwd lines)
        assert!(
            !data.contains('\n'),
            "Accepted GECOS with newline (passwd injection): {data:?}"
        );
        assert!(
            !data.contains('\r'),
            "Accepted GECOS with CR: {data:?}"
        );
        assert!(
            !data.contains('\0'),
            "Accepted GECOS with null byte: {data:?}"
        );
    }
});
