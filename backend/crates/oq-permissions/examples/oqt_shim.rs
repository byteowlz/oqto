//! Private exact-source enforcement helper; no installation or ambient shim lookup.
fn main() -> anyhow::Result<()> {
    oqto_sandbox::run_cli()
}
