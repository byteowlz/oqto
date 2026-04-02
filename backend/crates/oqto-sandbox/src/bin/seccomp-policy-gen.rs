use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::ffi::CString;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::path::{Path, PathBuf};

const SCMP_ACT_ALLOW: u32 = 0x7fff_0000;
const SCMP_ACT_ERRNO_BASE: u32 = 0x0005_0000;

#[derive(Parser, Debug)]
#[command(name = "seccomp-policy-gen")]
#[command(about = "Compile Oqto seccomp policy TOML into arch-specific BPF artifacts")]
struct Args {
    /// Path to policy TOML
    #[arg(long)]
    policy: PathBuf,

    /// Output directory for compiled *.bpf artifacts
    #[arg(long)]
    out_dir: PathBuf,

    /// Comma-separated architectures (e.g. x86_64,aarch64)
    #[arg(long, default_value = "x86_64,aarch64")]
    arches: String,
}

#[derive(Debug, Deserialize)]
struct PolicyFile {
    policy: Policy,
}

#[derive(Debug, Deserialize)]
struct Policy {
    default_action: String,
    errno: Option<u16>,
    classes: Vec<SyscallClass>,
}

#[derive(Debug, Deserialize)]
struct SyscallClass {
    syscalls: Vec<String>,
}

type ScmpFilterCtx = *mut c_void;

#[link(name = "seccomp")]
unsafe extern "C" {
    fn seccomp_init(def_action: u32) -> ScmpFilterCtx;
    fn seccomp_release(ctx: ScmpFilterCtx);
    fn seccomp_rule_add(
        ctx: ScmpFilterCtx,
        action: u32,
        syscall: c_int,
        arg_cnt: c_uint,
        ...
    ) -> c_int;
    fn seccomp_syscall_resolve_name(name: *const c_char) -> c_int;
    fn seccomp_export_bpf(ctx: ScmpFilterCtx, fd: c_int) -> c_int;
    fn seccomp_arch_resolve_name(arch_name: *const c_char) -> u32;
    fn seccomp_arch_native() -> u32;
    fn seccomp_arch_add(ctx: ScmpFilterCtx, arch_token: u32) -> c_int;
    fn seccomp_arch_remove(ctx: ScmpFilterCtx, arch_token: u32) -> c_int;
}

struct FilterCtx(ScmpFilterCtx);

impl Drop for FilterCtx {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: ctx was returned by seccomp_init and can be released once.
            unsafe { seccomp_release(self.0) };
        }
    }
}

fn errno_action(errno: u16) -> u32 {
    SCMP_ACT_ERRNO_BASE | u32::from(errno)
}

fn compile_for_arch(
    arch: &str,
    syscalls: &BTreeSet<String>,
    default_action: u32,
    out_path: &Path,
) -> Result<()> {
    // SAFETY: seccomp_init creates new filter context for the provided action.
    let raw_ctx = unsafe { seccomp_init(default_action) };
    if raw_ctx.is_null() {
        bail!("seccomp_init failed for arch {}", arch);
    }
    let ctx = FilterCtx(raw_ctx);

    let arch_c = CString::new(arch).context("arch CString")?;
    // SAFETY: valid C string pointer.
    let arch_token = unsafe { seccomp_arch_resolve_name(arch_c.as_ptr()) };
    if arch_token == 0 {
        bail!("unknown seccomp architecture token for '{}')", arch);
    }

    // SAFETY: pure query, no side effects.
    let native_arch = unsafe { seccomp_arch_native() };
    if arch_token != native_arch {
        // SAFETY: ctx valid, token resolved by libseccomp.
        let add_rc = unsafe { seccomp_arch_add(ctx.0, arch_token) };
        if add_rc != 0 {
            bail!("seccomp_arch_add({}) failed rc={}", arch, add_rc);
        }
        // SAFETY: remove native arch to produce single-arch artifact.
        let rm_rc = unsafe { seccomp_arch_remove(ctx.0, native_arch) };
        if rm_rc != 0 {
            bail!("seccomp_arch_remove(native) failed rc={}", rm_rc);
        }
    }

    for name in syscalls {
        let c_name = CString::new(name.as_str()).context("syscall CString")?;
        // SAFETY: valid C string pointer.
        let nr = unsafe { seccomp_syscall_resolve_name(c_name.as_ptr()) };
        if nr < 0 {
            bail!("unknown syscall in policy: {}", name);
        }

        // SAFETY: ctx/nr valid; arg_cnt=0 => no varargs consumed.
        let rc = unsafe { seccomp_rule_add(ctx.0, SCMP_ACT_ALLOW, nr, 0) };
        if rc != 0 {
            bail!("seccomp_rule_add failed for syscall '{}' rc={}", name, rc);
        }
    }

    let out = File::create(out_path)
        .with_context(|| format!("creating output file {}", out_path.display()))?;
    // SAFETY: export writes BPF to provided writable fd.
    let export_rc = unsafe { seccomp_export_bpf(ctx.0, out.as_raw_fd()) };
    if export_rc != 0 {
        bail!("seccomp_export_bpf failed rc={}", export_rc);
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let raw = std::fs::read_to_string(&args.policy)
        .with_context(|| format!("reading policy {}", args.policy.display()))?;
    let policy_file: PolicyFile =
        toml::from_str(&raw).with_context(|| "parsing seccomp policy toml")?;

    let default_action = match policy_file.policy.default_action.as_str() {
        "errno" => errno_action(policy_file.policy.errno.unwrap_or(1)),
        "allow" => SCMP_ACT_ALLOW,
        other => bail!("unsupported default_action '{}'", other),
    };

    let mut syscalls = BTreeSet::new();
    for class in &policy_file.policy.classes {
        for s in &class.syscalls {
            syscalls.insert(s.clone());
        }
    }

    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("creating out dir {}", args.out_dir.display()))?;

    for arch in args
        .arches
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let out_path = args.out_dir.join(format!("default-{}.bpf", arch));
        compile_for_arch(arch, &syscalls, default_action, &out_path)?;
        println!("wrote {}", out_path.display());
    }

    Ok(())
}
