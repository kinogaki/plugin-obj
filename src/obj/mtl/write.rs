//! MTL writing: each material's statements in a fixed order, values in their
//! shortest round-trip spelling.
use super::{MapOptions, MaterialLibrary};
use crate::obj::write::float;

/// The MTL text for `library`.
pub fn to_bytes(library: &MaterialLibrary) -> Vec<u8> {
    let mut text = Vec::new();
    let mut buffer = kinogaki_float_text::FloatText::new();
    for (index, material) in library.materials.iter().enumerate() {
        if index > 0 {
            text.push(b'\n');
        }
        line(&mut text, "newmtl", &material.name);
        let colors = [
            ("Ka", material.ambient),
            ("Kd", material.diffuse),
            ("Ks", material.specular),
            ("Ke", material.emissive),
            ("Tf", material.transmission_filter),
        ];
        for (keyword, color) in colors {
            if let Some(color) = color {
                text.extend_from_slice(keyword.as_bytes());
                for value in color {
                    text.push(b' ');
                    float(&mut text, &mut buffer, value);
                }
                text.push(b'\n');
            }
        }
        let scalars = [
            ("Ns", material.shininess),
            ("Ni", material.optical_density),
            ("d", material.dissolve),
            ("Tr", material.transparency),
            ("Pr", material.roughness),
            ("Pm", material.metallic),
            ("Ps", material.sheen),
            ("Pc", material.clearcoat),
            ("Pcr", material.clearcoat_roughness),
            ("aniso", material.anisotropy),
            ("anisor", material.anisotropy_rotation),
        ];
        for (keyword, value) in scalars {
            if let Some(value) = value {
                text.extend_from_slice(keyword.as_bytes());
                text.push(b' ');
                float(&mut text, &mut buffer, value);
                text.push(b'\n');
            }
        }
        if let Some(model) = material.illumination {
            line(&mut text, "illum", &model.to_string());
        }
        for map in &material.maps {
            text.extend_from_slice(map.kind.as_bytes());
            options(&mut text, &mut buffer, &map.options);
            text.push(b' ');
            text.extend_from_slice(map.path.as_bytes());
            text.push(b'\n');
        }
        for (keyword, value) in &material.other {
            line(&mut text, keyword, value);
        }
    }
    text
}

fn line(text: &mut Vec<u8>, keyword: &str, value: &str) {
    text.extend_from_slice(keyword.as_bytes());
    if !value.is_empty() {
        text.push(b' ');
        text.extend_from_slice(value.as_bytes());
    }
    text.push(b'\n');
}

/// A map's options, each with a space before it: `-o`, `-s`, `-mm`,
/// `-clamp`, `-imfchan`, `-bm`, then the others as read.
fn options(text: &mut Vec<u8>, buffer: &mut kinogaki_float_text::FloatText, o: &MapOptions) {
    let mut numbers = |text: &mut Vec<u8>, name: &str, values: &[f64]| {
        text.push(b' ');
        text.extend_from_slice(name.as_bytes());
        for &value in values {
            text.push(b' ');
            float(text, buffer, value);
        }
    };
    if let Some(offset) = o.offset {
        numbers(text, "-o", &offset);
    }
    if let Some(scale) = o.scale {
        numbers(text, "-s", &scale);
    }
    if let Some(range) = o.range {
        numbers(text, "-mm", &range);
    }
    let word = |text: &mut Vec<u8>, name: &str, value: &str| {
        text.extend_from_slice(format!(" {name} {value}").as_bytes());
    };
    if let Some(clamp) = o.clamp {
        word(text, "-clamp", if clamp { "on" } else { "off" });
    }
    if let Some(channel) = o.channel {
        word(text, "-imfchan", &channel.to_string());
    }
    if let Some(bump) = o.bump {
        numbers(text, "-bm", &[bump]);
    }
    for (name, values) in &o.other {
        word(text, name, &values.join(" "));
    }
}
