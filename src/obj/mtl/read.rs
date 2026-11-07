//! MTL reading: one sequential pass (material libraries are small).
use super::{MapOptions, Material, MaterialLibrary, TextureMap};
use crate::obj::{
    Limits, MalformedKind, ObjError,
    error::{Fault, limit},
    line::Line,
};
use std::{fs::File, io::Read, path::Path};

/// Parse MTL text. Statements before the first `newmtl` have no material to
/// belong to and are ignored.
pub fn parse(text: &[u8]) -> Result<MaterialLibrary, ObjError> {
    parse_with(text, &Limits::default())
}

pub fn parse_with(text: &[u8], limits: &Limits) -> Result<MaterialLibrary, ObjError> {
    if text.len() as u64 > limits.input_bytes {
        return Err(limit("input exceeds the size limit").into());
    }
    library(text, limits).map_err(|fault| fault.locate(text, 1).into())
}

/// Read an MTL file.
pub fn read_file(path: impl AsRef<Path>) -> Result<MaterialLibrary, ObjError> {
    let limits = Limits::default();
    let file = File::open(path)?;
    let mut text = Vec::new();
    file.take(limits.input_bytes + 1).read_to_end(&mut text)?;
    parse_with(&text, &limits)
}

/// The file names one `mtllib` value may mean: the whole value (names with
/// spaces), then each space-separated name (several libraries on one line).
pub fn library_paths(value: &str) -> Vec<&str> {
    let mut paths = vec![value];
    if value.contains(char::is_whitespace) {
        paths.extend(value.split_whitespace());
    }
    paths
}

fn library(text: &[u8], limits: &Limits) -> Result<MaterialLibrary, Fault> {
    let mut library = MaterialLibrary::default();
    let mut at = 0;
    while at < text.len() {
        let stop =
            kinogaki_bytes::find(b'\n', &text[at..]).map_or(text.len(), |offset| at + offset);
        if stop - at > limits.line_bytes {
            return Err(Fault::new(
                MalformedKind::Limit,
                at,
                "line exceeds the length limit",
            ));
        }
        let mut line = Line {
            bytes: text,
            at,
            end: stop,
        };
        if let Some(keyword) = line.token() {
            if keyword == b"newmtl" {
                library.materials.push(Material {
                    name: line.rest(),
                    ..Material::default()
                });
            } else if let Some(material) = library.materials.last_mut() {
                statement(material, keyword, &mut line)?;
            }
        }
        at = stop + 1;
    }
    Ok(library)
}

fn statement(material: &mut Material, keyword: &[u8], line: &mut Line<'_>) -> Result<(), Fault> {
    line.skip();
    let start = line.at;
    let scalar = |line: &mut Line<'_>| -> Result<Option<f64>, Fault> {
        let value = line.number()?;
        line.finish("expected one value")?;
        Ok(Some(value))
    };
    match keyword {
        b"Ka" | b"Kd" | b"Ks" | b"Ke" | b"Tf" => {
            let slot = match keyword {
                b"Ka" => &mut material.ambient,
                b"Kd" => &mut material.diffuse,
                b"Ks" => &mut material.specular,
                b"Ke" => &mut material.emissive,
                _ => &mut material.transmission_filter,
            };
            // `spectral file` and `xyz x y z` forms are kept verbatim.
            if line.more()
                && !line.bytes[line.at].is_ascii_digit()
                && !b"+-.".contains(&line.bytes[line.at])
            {
                return other(material, keyword, line);
            }
            let r = line.number()?;
            let color = if line.more() {
                [r, line.number()?, line.number()?]
            } else {
                [r; 3]
            };
            line.finish("a color takes one or three values")?;
            *slot = Some(color);
        }
        b"Ns" => material.shininess = scalar(line)?,
        b"Ni" => material.optical_density = scalar(line)?,
        b"d" => {
            // `d -halo f` is kept verbatim.
            if line.more()
                && line.bytes[line.at] == b'-'
                && line.bytes.get(line.at + 1) == Some(&b'h')
            {
                return other(material, keyword, line);
            }
            material.dissolve = scalar(line)?;
        }
        b"Tr" => material.transparency = scalar(line)?,
        b"Pr" => material.roughness = scalar(line)?,
        b"Pm" => material.metallic = scalar(line)?,
        b"Ps" => material.sheen = scalar(line)?,
        b"Pc" => material.clearcoat = scalar(line)?,
        b"Pcr" => material.clearcoat_roughness = scalar(line)?,
        b"aniso" => material.anisotropy = scalar(line)?,
        b"anisor" => material.anisotropy_rotation = scalar(line)?,
        b"illum" => {
            let token = line.token().unwrap_or_default();
            let value = std::str::from_utf8(token)
                .ok()
                .and_then(|text| text.parse().ok())
                .ok_or_else(|| {
                    Fault::new(
                        MalformedKind::Syntax,
                        start,
                        "expected an illumination model number",
                    )
                })?;
            line.finish("illum takes one value")?;
            material.illumination = Some(value);
        }
        _ if is_map(keyword) => {
            let map = texture(keyword, line)?;
            if map.path.is_empty() {
                return Err(Fault::new(
                    MalformedKind::Syntax,
                    start,
                    "a texture map needs a file name",
                ));
            }
            material.maps.push(map);
        }
        _ => return other(material, keyword, line),
    }
    Ok(())
}

fn other(material: &mut Material, keyword: &[u8], line: &mut Line<'_>) -> Result<(), Fault> {
    let keyword = String::from_utf8_lossy(keyword).into_owned();
    material.other.push((keyword, line.rest()));
    Ok(())
}

fn is_map(keyword: &[u8]) -> bool {
    keyword.starts_with(b"map_")
        || matches!(keyword, b"bump" | b"disp" | b"decal" | b"refl" | b"norm")
}

/// How many values option `option` takes: `(least, most)`; numbers alone
/// for `-o`, `-s`, `-t` and `-mm`. None: not an option.
fn option_values(option: &[u8]) -> Option<(usize, usize, bool)> {
    match option {
        b"-blendu" | b"-blendv" | b"-cc" | b"-clamp" | b"-imfchan" | b"-type" | b"-texres"
        | b"-bm" | b"-boost" => Some((1, 1, false)),
        b"-mm" => Some((2, 2, true)),
        b"-o" | b"-s" | b"-t" => Some((1, 3, true)),
        _ => None,
    }
}

/// `keyword [options] file name`.
fn texture(keyword: &[u8], line: &mut Line<'_>) -> Result<TextureMap, Fault> {
    let mut options = MapOptions::default();
    loop {
        let before = line.at;
        let Some(token) = line.token() else { break };
        let Some((least, most, numeric)) = option_values(token) else {
            line.at = before;
            break;
        };
        let mut values: Vec<String> = Vec::new();
        while values.len() < most {
            let mark = line.at;
            let Some(value) = line.token() else { break };
            let number = std::str::from_utf8(value).is_ok_and(|t| t.parse::<f64>().is_ok());
            // A word ends a run of numbers: the file name may follow.
            if numeric && !number {
                line.at = mark;
                break;
            }
            values.push(String::from_utf8_lossy(value).into_owned());
        }
        let name = String::from_utf8_lossy(token).into_owned();
        if values.len() < least {
            return Err(Fault::new(
                MalformedKind::Syntax,
                before,
                "a texture option is missing its values",
            ));
        }
        option(&mut options, name, values);
    }
    Ok(TextureMap {
        kind: String::from_utf8_lossy(keyword).into_owned(),
        options,
        path: line.rest(),
    })
}

/// Option `name` with `values` kept: as its field where it reads as one,
/// else verbatim among the others.
fn option(options: &mut MapOptions, name: String, values: Vec<String>) {
    let numbers: Option<Vec<f64>> = values.iter().map(|v| v.parse().ok()).collect();
    let triple = |fill: f64| {
        let n = numbers.as_deref()?;
        Some([0, 1, 2].map(|k| n.get(k).copied().unwrap_or(fill)))
    };
    let switch = match values.first().map(String::as_str) {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    };
    let kept = match name.as_str() {
        "-o" => triple(0.0).map(|o| options.offset = Some(o)),
        "-s" => triple(1.0).map(|s| options.scale = Some(s)),
        "-mm" => numbers
            .as_deref()
            .map(|n| options.range = Some([n[0], n[1]])),
        "-clamp" => switch.map(|on| options.clamp = Some(on)),
        "-bm" => numbers.as_deref().map(|n| options.bump = Some(n[0])),
        "-imfchan" => match values[0].as_str() {
            c @ ("r" | "g" | "b" | "m" | "l" | "z") => {
                options.channel = c.chars().next();
                Some(())
            }
            _ => None,
        },
        _ => None,
    };
    if kept.is_none() {
        options.other.push((name, values));
    }
}
