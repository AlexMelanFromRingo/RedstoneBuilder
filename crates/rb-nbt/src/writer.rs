//! gzip + fastnbt streaming writer for `.litematic`.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use flate2::write::GzEncoder;
use flate2::Compression;

use crate::error::NbtError;
use crate::litematic::{build_root, LitematicaRoot};
use crate::BlockGrid;

/// Write `grid` to a `.litematic` file at `path`.
///
/// Output is **gzip-compressed NBT** (RFC 1952). Determinism (FR-017):
/// fixed compression level + first-encounter palette ordering +
/// zeroed timestamps ⇒ byte-identical output for identical input.
pub fn write_litematic(
    path: &Path,
    grid: &BlockGrid,
    name: &str,
    description: &str,
) -> Result<(), NbtError> {
    let root = build_root(grid, name, description);
    let bytes = encode_to_bytes(&root)?;

    let file = File::create(path)?;
    let mut bw = BufWriter::new(file);
    bw.write_all(&bytes)?;
    bw.flush()?;
    Ok(())
}

/// Encode a [`LitematicaRoot`] to the gzipped NBT byte stream used in
/// `.litematic` files. Exposed so the round-trip test can verify
/// without touching the filesystem.
pub fn encode_to_bytes(root: &LitematicaRoot) -> Result<Vec<u8>, NbtError> {
    let nbt = fastnbt::to_bytes(root)?;
    let mut gz = GzEncoder::new(Vec::with_capacity(nbt.len() / 2), Compression::default());
    gz.write_all(&nbt)?;
    let bytes = gz.finish()?;
    Ok(bytes)
}

/// Decode a gzipped Litematica byte stream back into a
/// [`LitematicaRoot`]. Exposed for round-trip testing.
pub fn decode_from_bytes(bytes: &[u8]) -> Result<LitematicaRoot, NbtError> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut gz = GzDecoder::new(bytes);
    let mut raw = Vec::new();
    gz.read_to_end(&mut raw)?;
    let root: LitematicaRoot = fastnbt::from_bytes(&raw)?;
    Ok(root)
}
