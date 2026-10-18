fn main() -> anyhow::Result<()> {
    println!("{}", oqto_sandbox::SandboxConfigFile::schema_json()?);
    Ok(())
}
