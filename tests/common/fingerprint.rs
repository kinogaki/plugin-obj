//! A short fingerprint of a model's mesh, for the differential tests and the
//! corpus tool.

use kinogaki_obj::obj::{NONE, Obj};

/// The fingerprint of `obj`'s mesh with `points` as its positions: the
/// points, polygons, corner `uv` (u, v) and `N` when any face references them
/// (0 where a corner does not).
pub fn hash(obj: &Obj, points: &[[f64; 3]]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut word = |value: u32| {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
    };
    let float = |word: &mut dyn FnMut(u32), value: f64| word((value as f32).to_bits());
    for point in points {
        for &value in point {
            let bits = value.to_bits();
            word(bits as u32);
            word((bits >> 32) as u32);
        }
    }
    let faces = &obj.faces;
    for face in 0..faces.len() {
        word(faces.range(face).len() as u32);
        for &corner in faces.corners(face) {
            word(corner);
        }
    }
    let gather = |indices: &[u32], table: &[[f64; 3]], width: usize| -> Vec<f64> {
        indices
            .iter()
            .flat_map(|&at| {
                let value = if at == NONE {
                    [0.0; 3]
                } else {
                    table[at as usize]
                };
                value[..width].to_vec()
            })
            .collect()
    };
    let uv = gather(&faces.texcoords, &obj.texcoords, 2);
    let normals = gather(&faces.normals, &obj.normals, 3);
    for &value in uv.iter().chain(&normals) {
        float(&mut word, value);
    }
    format!(
        "{} points {} faces {} corners uv={} N={} {:016x}",
        obj.positions.len(),
        faces.len(),
        faces.corner_count(),
        !uv.is_empty(),
        !normals.is_empty(),
        hash
    )
}
