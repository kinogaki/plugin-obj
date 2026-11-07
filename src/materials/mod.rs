//! MTL materials as MaterialX networks and back: the two shading models a
//! material library records, statement by statement.
//!
//! - **Classic statements** (`Ka`, `Kd`, `Ks`, `Ke`, `Tf`, `Ns`, `Ni`, `d`,
//!   `Tr`, `illum`, their maps) are a Phong Surface
//!   (`kinogaki_phong_surface`).
//! - **The PBR extension** (`Pr`, `Pm`, `Ps`, `Pc`, `Pcr`, `aniso`, `anisor`,
//!   `norm`, their maps) makes a material Autodesk's Standard Surface, which
//!   takes `Kd`, `Ke`, `Ni`, `d` and `Tf` too; the classic statements it
//!   doesn't take (Blender writes both) go to a Phong Surface beside it that
//!   nothing reads.
//! - **A map** is an Image node wired into an `image` node of the input's
//!   type, multiplied by its statement's value where the material has both
//!   (`Kd` and `map_Kd`). Its options: `-s` and `-o` scale and then move the
//!   texture coordinates (`multiply`, `add`), `-clamp on` clamps both address
//!   modes, `-imfchan` extracts a channel (`m` the alpha), `-mm base gain`
//!   multiplies by the gain and adds the base.
//! - **Bumps and normals:** `bump`/`map_Bump` is a `bump` node of heights
//!   into the surface's normal, its `-bm` the scale; read as a normal map
//!   when the reader is told Blender's convention (D19); `norm` is a
//!   `normalmap`. `disp` is a `displacement` into a Surface Material.
//!
//! - **Colors** are linear Rec.709, read into the working space and written
//!   out of it.
//!
//! [`read::materials`] makes the networks of a library; [`write::library`]
//! makes a library of networks: Phong and Standard Surface statements as
//! they are recorded, any other model translated to Standard Surface.
pub mod read;
pub mod write;

/// How a statement's value is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Three numbers: a color3 input.
    Color,
    /// One number: a float input.
    Number,
    /// One number into a color3 input as a grey (`d` into Standard Surface's
    /// `opacity`).
    Grey,
    /// A whole number: an integer input (`illum`).
    Integer,
}

/// A statement of a shading model: its keyword, the map statement reading
/// the same input (empty: none), and the input.
#[derive(Clone, Copy, Debug)]
struct Statement {
    keyword: &'static str,
    map: &'static str,
    input: &'static str,
    kind: Kind,
}

const fn statement(
    keyword: &'static str,
    map: &'static str,
    input: &'static str,
    kind: Kind,
) -> Statement {
    Statement {
        keyword,
        map,
        input,
        kind,
    }
}

/// The Phong Surface's signature and its statements (`Tr` apart).
const PHONG: &str = "ND_kinogaki_phong_surface_surfaceshader";
const PHONG_STATEMENTS: [Statement; 10] = [
    statement("Ka", "map_Ka", "ambient_color", Kind::Color),
    statement("Kd", "map_Kd", "diffuse_color", Kind::Color),
    statement("Ks", "map_Ks", "specular_color", Kind::Color),
    statement("Ke", "map_Ke", "emissive_color", Kind::Color),
    statement("Tf", "", "transmission_filter", Kind::Color),
    statement("Ns", "map_Ns", "shininess", Kind::Number),
    statement("Ni", "", "ior", Kind::Number),
    statement("d", "map_d", "opacity", Kind::Number),
    statement("illum", "", "illum", Kind::Integer),
    statement("", "map_refl", "reflection_color", Kind::Color),
];

/// Standard Surface's signature and its statements.
const STANDARD: &str = "ND_standard_surface_surfaceshader";
const STANDARD_STATEMENTS: [Statement; 12] = [
    statement("Kd", "map_Kd", "base_color", Kind::Color),
    statement("Ke", "map_Ke", "emission_color", Kind::Color),
    statement("Ni", "", "specular_IOR", Kind::Number),
    statement("d", "map_d", "opacity", Kind::Grey),
    statement("Tf", "", "transmission_color", Kind::Color),
    statement("Pr", "map_Pr", "specular_roughness", Kind::Number),
    statement("Pm", "map_Pm", "metalness", Kind::Number),
    statement("Ps", "map_Ps", "sheen", Kind::Number),
    statement("Pc", "map_Pc", "coat", Kind::Number),
    statement("Pcr", "map_Pcr", "coat_roughness", Kind::Number),
    statement("aniso", "map_aniso", "specular_anisotropy", Kind::Number),
    statement("anisor", "map_anisor", "specular_rotation", Kind::Number),
];

/// Standard Surface's weights the statements' colors are read at: `Kd` is
/// the base color at a base of 1, `Ke` the emission color at an emission of 1.
const WEIGHTS: [(&str, &str); 2] = [("base_color", "base"), ("emission_color", "emission")];

/// The classic statements a Standard Surface material keeps beside it, on a
/// Phong Surface nothing reads: those Standard Surface doesn't take.
fn beside(s: &Statement) -> bool {
    !STANDARD_STATEMENTS
        .iter()
        .any(|t| t.keyword == s.keyword && t.map == s.map)
}

/// The statements and maps only the PBR extension has: a material holding
/// any is a Standard Surface.
const PBR_ONLY: [&str; 15] = [
    "Pr",
    "Pm",
    "Ps",
    "Pc",
    "Pcr",
    "aniso",
    "anisor",
    "map_Pr",
    "map_Pm",
    "map_Ps",
    "map_Pc",
    "map_Pcr",
    "map_aniso",
    "map_anisor",
    "norm",
];

/// The map keywords a height map is read from, and the normal map's.
const BUMP: [&str; 2] = ["map_Bump", "bump"];
const NORMAL: &str = "norm";
const DISPLACEMENT: &str = "disp";

/// MaterialX's nodes the mapping makes, by signature.
mod sig {
    pub const SURFACE_MATERIAL: &str = "ND_surfacematerial";
    pub const TEXCOORD: &str = "ND_texcoord_vector2";
    pub const MULTIPLY_UV: &str = "ND_multiply_vector2";
    pub const ADD_UV: &str = "ND_add_vector2";
    pub const MULTIPLY_COLOR: &str = "ND_multiply_color3";
    pub const MULTIPLY_COLOR_BY: &str = "ND_multiply_color3FA";
    pub const ADD_COLOR_BY: &str = "ND_add_color3FA";
    pub const MULTIPLY: &str = "ND_multiply_float";
    pub const ADD: &str = "ND_add_float";
    pub const EXTRACT_COLOR3: &str = "ND_extract_color3";
    pub const EXTRACT_COLOR4: &str = "ND_extract_color4";
    pub const GREY: &str = "ND_convert_float_color3";
    pub const INVERT: &str = "ND_invert_float";
    pub const NORMAL_MAP: &str = "ND_normalmap_float";
    pub const BUMP: &str = "ND_bump_vector3";
    pub const DISPLACEMENT: &str = "ND_displacement_float";
}

#[cfg(test)]
mod tests {
    use super::*;
    use kinogaki_render_api::material::mtlx::def;

    #[test]
    fn signatures_the_mapping_makes_are_materialx_own() {
        for signature in [
            PHONG,
            STANDARD,
            sig::SURFACE_MATERIAL,
            sig::TEXCOORD,
            sig::MULTIPLY_UV,
            sig::ADD_UV,
            sig::MULTIPLY_COLOR,
            sig::MULTIPLY_COLOR_BY,
            sig::ADD_COLOR_BY,
            sig::MULTIPLY,
            sig::ADD,
            sig::EXTRACT_COLOR3,
            sig::EXTRACT_COLOR4,
            sig::GREY,
            sig::INVERT,
            sig::NORMAL_MAP,
            sig::BUMP,
            sig::DISPLACEMENT,
        ] {
            assert!(def(signature).is_some(), "{signature}");
        }
    }
}
