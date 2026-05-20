//! Convert a decoded Sponge `.schem` into a [`Stub`] per the
//! redhdl convention. Ported from
//! `redhdl/netlist/schematic_instance.py::schematic_instance_from_schem`
//! with our four patches baked in:
//!
//! - Glass markers may be ANY glass variant (plain, stained, tinted).
//! - Glass corners are NOT required to sit at AABB extrema — they
//!   may lie on any of the 4 main diagonals.
//! - Schematic-name sign is found by probing all 6 faces of
//!   `bottom_right_pos` (different authors use different orientations).
//! - Port-pin signs scanned across the whole schematic, filtered by
//!   the `"input "` / `"output "` prefix (not by "outside-padded-region").

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use rb_core::{Bbox3, Direction, Pos3};
use rb_nbt::{decode_sponge_schem, SpongeSchematic};
use smol_str::SmolStr;

use crate::error::StubError;
use crate::stub::{PortDir, Stub, StubLibrary, StubPort};

/// Manhattan (L1) distance between two stub-local positions.
fn manhattan(a: Pos3, b: Pos3) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

/// Knobs controlling how aggressive the loader is when reading
/// non-canonical stubs.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoadOptions {
    /// If `true`, fall back to the schematic bbox when no glass
    /// markers exist at all (rather than erroring out). Useful for
    /// minimal hand-crafted stubs.
    pub allow_no_glass: bool,
}

/// Scan a directory tree for `*.schem` (and later `*.litematic`)
/// files and load every valid stub. Files that fail to parse are
/// skipped with a stderr warning so a bad single stub doesn't
/// break library bootstrap.
pub fn load_stub_library(root: &Path) -> Result<StubLibrary, StubError> {
    let mut lib = StubLibrary::new();
    visit(root, &mut |path| {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext == "schem" {
            match load_stub_from_schem(path, LoadOptions::default()) {
                Ok(stub) => lib.insert(stub),
                Err(e) => eprintln!("[stub-loader] skipping {}: {e}", path.display()),
            }
        }
        // TODO: add .litematic loader when rb-nbt grows a reader.
    })?;
    Ok(lib)
}

fn visit(root: &Path, f: &mut dyn FnMut(&Path)) -> Result<(), StubError> {
    let entries = fs::read_dir(root).map_err(|e| StubError::Io {
        path: root.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| StubError::Io {
            path: root.to_path_buf(),
            source: e,
        })?;
        let p = entry.path();
        if p.is_dir() {
            visit(&p, f)?;
        } else {
            f(&p);
        }
    }
    Ok(())
}

/// Read and convert a single Sponge `.schem` into a [`Stub`].
pub fn load_stub_from_schem(path: &Path, opts: LoadOptions) -> Result<Stub, StubError> {
    let bytes = fs::read(path).map_err(|e| StubError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let schem = decode_sponge_schem(&bytes).map_err(|e| StubError::BadNbt {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    stub_from_sponge(path, &schem, opts)
}

/// Pure-data conversion (no I/O). Useful for tests.
pub fn stub_from_sponge(
    path: &Path,
    schem: &SpongeSchematic,
    opts: LoadOptions,
) -> Result<Stub, StubError> {
    let (br, tl) = corner_positions(schem, opts).map_err(|message| StubError::NotAStub {
        path: path.to_path_buf(),
        message,
    })?;
    let bbox_min = Pos3::new(br.x.min(tl.x), br.y.min(tl.y), br.z.min(tl.z));
    let bbox_max = Pos3::new(br.x.max(tl.x), br.y.max(tl.y), br.z.max(tl.z));
    let bbox = Bbox3::from_corners(bbox_min, bbox_max);

    // Schematic-name sign: probe all 6 faces of BR for a non-port sign.
    let name_sign_pos = find_name_sign_near(schem, br).ok_or_else(|| StubError::NotAStub {
        path: path.to_path_buf(),
        message: format!("no schematic-name sign adjacent to glass corner at {br:?}"),
    })?;
    let name_lines = sign_lines_at(schem, name_sign_pos).unwrap_or_default();
    let name = name_lines
        .first()
        .cloned()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .replace(' ', "_");

    // Collect port-pin signs from the WHOLE schematic; filter by
    // "input "/"output " prefix.
    let mut pin_signs: Vec<(Pos3, PortDir, String, u32, Direction)> = Vec::new();
    for be in &schem.block_entities {
        if !be.id.as_str().contains("sign") {
            continue;
        }
        let lines = be.sign_text();
        let first = match lines.first() {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };
        let parsed = match parse_port_sign(&first) {
            Some(p) => p,
            None => continue,
        };
        // Sign facing — needed to orient the pin. Default to North
        // if the sign block has no facing property (shouldn't happen
        // for wall signs but be defensive).
        let facing = schem
            .blocks
            .get(&be.pos)
            .and_then(|bs| bs.properties.get("facing"))
            .and_then(|f| parse_direction(f.as_str()))
            .unwrap_or(Direction::North);
        pin_signs.push((be.pos, parsed.0, parsed.1, parsed.2, facing));
    }

    // Group pin signs by (dir, name).
    type PortKey = (PortDir, SmolStr);
    type PinRecord = (u32, Pos3, Direction);
    let mut grouped: BTreeMap<PortKey, Vec<PinRecord>> = BTreeMap::new();
    for (pos, dir, name, idx, facing) in pin_signs {
        // The pin block sits 1 cell behind the sign (sign mounts on a
        // block face). For a sign facing direction D, the mounting
        // block is at sign_pos - D-offset.
        let (dx, dy, dz) = facing.offset();
        let pin_pos = Pos3::new(pos.x - dx, pos.y - dy, pos.z - dz);
        grouped
            .entry((dir, SmolStr::new(name)))
            .or_default()
            .push((idx, pin_pos, facing));
    }

    let mut ports: Vec<StubPort> = Vec::new();
    for ((dir, name), mut pins) in grouped {
        pins.sort_by_key(|(idx, _, _)| *idx);
        let width = pins.last().map(|(idx, _, _)| *idx + 1).unwrap_or(1);
        let pin_start = pins
            .iter()
            .find(|(idx, _, _)| *idx == 0)
            .map(|(_, p, _)| *p)
            .or_else(|| pins.first().map(|(_, p, _)| *p))
            .unwrap_or(Pos3::ORIGIN);
        let pin_step = if pins.len() >= 2 {
            let (i0, p0, _) = pins[0];
            let (i1, p1, _) = pins[1];
            let delta = i1.saturating_sub(i0).max(1) as i32;
            Pos3::new(
                (p1.x - p0.x) / delta,
                (p1.y - p0.y) / delta,
                (p1.z - p0.z) / delta,
            )
        } else {
            Pos3::ORIGIN
        };
        let facing = pins.first().map(|(_, _, f)| *f).unwrap_or(Direction::North);
        ports.push(StubPort {
            name,
            dir,
            width,
            pin_start,
            pin_step,
            facing,
        });
    }

    // Core blocks = schematic blocks minus the 2 glass corner cells
    // and minus any sign cells (signs are metadata, not circuit).
    let sign_positions: std::collections::BTreeSet<Pos3> = schem
        .block_entities
        .iter()
        .filter(|be| be.id.as_str().contains("sign"))
        .map(|be| be.pos)
        .collect();
    let core_blocks: BTreeMap<Pos3, _> = schem
        .blocks
        .iter()
        .filter(|(pos, _)| **pos != br && **pos != tl && !sign_positions.contains(pos))
        .map(|(p, b)| (*p, b.clone()))
        .collect();

    Ok(Stub {
        name: SmolStr::new(name),
        bbox,
        ports,
        blocks: core_blocks,
    })
}

// ─────────────────────────────────────────────────────────────────────
// Internal helpers — corner detection, sign parsing, etc.
// ─────────────────────────────────────────────────────────────────────

/// Return `(bottom_right, top_left)` of the stub's padded region.
/// See module-level docstring for the rules.
fn corner_positions(schem: &SpongeSchematic, opts: LoadOptions) -> Result<(Pos3, Pos3), String> {
    let glass: Vec<Pos3> = schem
        .blocks
        .iter()
        .filter(|(_, b)| b.name.as_str().contains("glass"))
        .map(|(p, _)| *p)
        .collect();

    if glass.is_empty() {
        if !opts.allow_no_glass {
            return Err("no glass marker blocks (any glass variant)".to_string());
        }
        // Fall back to schematic bbox.
        let max_x = (schem.width as i32).saturating_sub(1);
        let max_y = (schem.height as i32).saturating_sub(1);
        let max_z = (schem.length as i32).saturating_sub(1);
        return Ok((Pos3::ORIGIN, Pos3::new(max_x, max_y, max_z)));
    }

    // Find name sign (non-port, non-credit) — needed to disambiguate
    // which glass corner is "bottom right". If we can't find a name
    // sign, fall back to elem-min/max of glass positions.
    let name_sign_candidates: Vec<Pos3> = schem
        .block_entities
        .iter()
        .filter(|be| be.id.as_str().contains("sign"))
        .filter_map(|be| {
            let lines = be.sign_text();
            let first = lines.first()?;
            if first.is_empty()
                || first.starts_with("input ")
                || first.starts_with("output ")
                || first.starts_with("Created by")
            {
                None
            } else {
                Some(be.pos)
            }
        })
        .collect();

    if let Some(&name_sign_pos) = name_sign_candidates.iter().min_by_key(|sp| {
        glass
            .iter()
            .map(|g| manhattan(**sp, *g))
            .min()
            .unwrap_or(i32::MAX)
    }) {
        // BR = glass block adjacent (within 1 cell) to the name sign.
        let br = glass
            .iter()
            .copied()
            .find(|g| manhattan(*g, name_sign_pos) <= 1);
        if let Some(br) = br {
            // TL = the glass block farthest from BR.
            let tl = glass
                .iter()
                .copied()
                .filter(|g| *g != br)
                .max_by_key(|g| manhattan(*g, br))
                .unwrap_or(br);
            return Ok((br, tl));
        }
    }

    // No name sign discoverable, or name sign isn't adjacent to any
    // glass. Fall back to elem-wise min/max.
    let br = Pos3::new(
        glass.iter().map(|p| p.x).min().unwrap_or(0),
        glass.iter().map(|p| p.y).min().unwrap_or(0),
        glass.iter().map(|p| p.z).min().unwrap_or(0),
    );
    let tl = Pos3::new(
        glass.iter().map(|p| p.x).max().unwrap_or(0),
        glass.iter().map(|p| p.y).max().unwrap_or(0),
        glass.iter().map(|p| p.z).max().unwrap_or(0),
    );
    Ok((br, tl))
}

/// Probe all 6 face-adjacent positions to `br` for a sign whose
/// first line is a non-port label. Returns the sign's position if
/// found. Falls back to scanning ALL signs in the schematic for a
/// non-port label if no adjacent sign matches (handles stubs whose
/// author placed the name sign farther from the glass).
fn find_name_sign_near(schem: &SpongeSchematic, br: Pos3) -> Option<Pos3> {
    let offsets = [
        Pos3::new(0, 0, -1),
        Pos3::new(0, 0, 1),
        Pos3::new(-1, 0, 0),
        Pos3::new(1, 0, 0),
        Pos3::new(0, -1, 0),
        Pos3::new(0, 1, 0),
    ];
    for off in offsets {
        let candidate = Pos3::new(br.x + off.x, br.y + off.y, br.z + off.z);
        let Some(be) = schem
            .block_entities
            .iter()
            .find(|be| be.id.as_str().contains("sign") && be.pos == candidate)
        else {
            continue;
        };
        let lines = be.sign_text();
        let Some(first) = lines.first() else { continue };
        if !first.is_empty()
            && !first.starts_with("input ")
            && !first.starts_with("output ")
            && !first.starts_with("Created by")
        {
            return Some(candidate);
        }
    }
    // Fallback: any non-port sign in the schematic, preferring the
    // one nearest to BR.
    let mut best: Option<(i32, Pos3)> = None;
    for be in &schem.block_entities {
        if !be.id.as_str().contains("sign") {
            continue;
        }
        let lines = be.sign_text();
        let Some(first) = lines.first() else { continue };
        if first.is_empty()
            || first.starts_with("input ")
            || first.starts_with("output ")
            || first.starts_with("Created by")
        {
            continue;
        }
        let d = manhattan(be.pos, br);
        if best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, be.pos));
        }
    }
    best.map(|(_, p)| p)
}

fn sign_lines_at(schem: &SpongeSchematic, pos: Pos3) -> Option<Vec<String>> {
    schem
        .block_entities
        .iter()
        .find(|be| be.pos == pos && be.id.as_str().contains("sign"))
        .map(|be| be.sign_text())
}

/// Parse `"input a[3]"` / `"output sum[0]"` / `"input clk"` into
/// `(dir, name, index)`. Returns `None` if the text doesn't look like
/// a port-pin sign.
fn parse_port_sign(s: &str) -> Option<(PortDir, String, u32)> {
    let (dir_kw, rest) = if let Some(r) = s.strip_prefix("input ") {
        (PortDir::In, r)
    } else if let Some(r) = s.strip_prefix("output ") {
        (PortDir::Out, r)
    } else {
        return None;
    };
    // rest is `<name>` or `<name>[<idx>]`.
    let (name, idx) = match rest.find('[') {
        Some(bi) => {
            let end = rest.rfind(']')?;
            let idx: u32 = rest[bi + 1..end].trim().parse().ok()?;
            (rest[..bi].trim().to_string(), idx)
        }
        None => (rest.trim().to_string(), 0),
    };
    Some((dir_kw, name, idx))
}

fn parse_direction(s: &str) -> Option<Direction> {
    match s.to_ascii_lowercase().as_str() {
        "north" => Some(Direction::North),
        "south" => Some(Direction::South),
        "east" => Some(Direction::East),
        "west" => Some(Direction::West),
        "up" => Some(Direction::Up),
        "down" => Some(Direction::Down),
        _ => None,
    }
}
