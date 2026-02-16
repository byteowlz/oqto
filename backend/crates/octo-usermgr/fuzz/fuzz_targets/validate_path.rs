#![no_main]
use libfuzzer_sys::fuzz_target;
use octo_usermgr::validate::*;

const ALLOWED_PREFIXES: &[&str] = &["/run/octo/runner-sockets/", "/home/octo_"];

fuzz_target!(|data: &str| {
    let result = validate_path(data, ALLOWED_PREFIXES);

    // Invariant: if validation passes, path must be safe
    if result.is_ok() {
        assert!(data.starts_with('/'), "Accepted relative path: {data:?}");
        assert!(
            !data.contains(".."),
            "Accepted path with traversal: {data:?}"
        );
        assert!(
            !data.contains("//"),
            "Accepted path with double slash: {data:?}"
        );
        assert!(
            !data.contains('\0'),
            "Accepted path with null byte: {data:?}"
        );
        assert!(
            !data.chars().any(|c| c.is_control()),
            "Accepted path with control chars: {data:?}"
        );

        // Must match at least one allowed prefix
        let has_prefix = ALLOWED_PREFIXES.iter().any(|p| data.starts_with(p));
        assert!(has_prefix, "Accepted path without allowed prefix: {data:?}");

        // Must NOT point to sensitive system paths
        assert!(
            !data.starts_with("/etc"),
            "Accepted /etc path: {data:?}"
        );
        assert!(
            !data.starts_with("/root"),
            "Accepted /root path: {data:?}"
        );
        assert!(
            !data.starts_with("/home/tommy"),
            "Accepted non-octo home path: {data:?}"
        );
    }
});
