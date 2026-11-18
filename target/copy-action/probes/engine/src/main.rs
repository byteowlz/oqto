// Throwaway manifest/CPU probe, not the proposed transfer implementation.
use std::{
    env, fs,
    io::{self, Write},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let data = fs::read(&args[2])?;
    let start = Instant::now();
    let mut out = io::BufWriter::new(io::stdout().lock());
    if args[1] == "hash" {
        let mut hash = blake3::Hash::from([0; 32]);
        for _ in 0..20 {
            hash = blake3::hash(&data);
        }
        writeln!(out, "{}", hash)?;
        eprintln!(
            "hash20_bytes={} seconds={:.6}",
            data.len() * 20,
            start.elapsed().as_secs_f64()
        );
    } else if args[1] == "cdc" {
        for chunk in fastcdc::v2020::FastCDC::new(&data, 65536, 262144, 1048576) {
            writeln!(
                out,
                "{}\t{}\t{}",
                chunk.offset,
                chunk.length,
                blake3::hash(&data[chunk.offset..chunk.offset + chunk.length])
            )?;
        }
        out.flush()?;
        eprintln!(
            "cdc_bytes={} seconds={:.6}",
            data.len(),
            start.elapsed().as_secs_f64()
        );
    } else if args[1] == "fixed" {
        for (index, chunk) in data.chunks(262144).enumerate() {
            writeln!(
                out,
                "{}\t{}\t{}",
                index * 262144,
                chunk.len(),
                blake3::hash(chunk)
            )?;
        }
        out.flush()?;
        eprintln!(
            "fixed_bytes={} seconds={:.6}",
            data.len(),
            start.elapsed().as_secs_f64()
        );
    } else {
        return Err("expected hash, cdc or fixed".into());
    }
    Ok(())
}
