#![no_main]
use libfuzzer_sys::fuzz_target;
use oqto_usermgr::validate::*;

fuzz_target!(|data: &str| {
    let result = validate_username(data);

    // Invariant: if validation passes, all security properties must hold
    if result.is_ok() {
        assert!(
            data.starts_with(USERNAME_PREFIX),
            "Accepted username without prefix: {data:?}"
        );
        assert!(
            data.len() <= USERNAME_MAX_LEN,
            "Accepted username too long: {data:?}"
        );
        assert!(
            data.len() > USERNAME_PREFIX.len(),
            "Accepted empty username after prefix: {data:?}"
        );
        assert!(
            data.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-'),
            "Accepted username with invalid chars: {data:?}"
        );
        // Must not contain shell metacharacters
        assert!(
            !data.contains(';'),
            "Accepted username with semicolon: {data:?}"
        );
        assert!(
            !data.contains('|'),
            "Accepted username with pipe: {data:?}"
        );
        assert!(
            !data.contains('$'),
            "Accepted username with dollar: {data:?}"
        );
        assert!(
            !data.contains('`'),
            "Accepted username with backtick: {data:?}"
        );
        assert!(
            !data.contains('\0'),
            "Accepted username with null byte: {data:?}"
        );
        assert!(
            !data.contains('\n'),
            "Accepted username with newline: {data:?}"
        );
    }
});
