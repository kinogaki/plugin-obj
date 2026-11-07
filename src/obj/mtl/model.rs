//! Material library data: the classic Phong-style statements, the common PBR
//! extension and texture maps with their options. Statements not modelled
//! here are kept in [`Material::other`], and options in
//! [`MapOptions::other`], so a read-write round trip keeps them.

/// A `.mtl` file: materials in file order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MaterialLibrary {
    pub materials: Vec<Material>,
}

/// One `newmtl` block.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    pub name: String,
    /// `Ka`
    pub ambient: Option<[f64; 3]>,
    /// `Kd`
    pub diffuse: Option<[f64; 3]>,
    /// `Ks`
    pub specular: Option<[f64; 3]>,
    /// `Ke`
    pub emissive: Option<[f64; 3]>,
    /// `Tf`
    pub transmission_filter: Option<[f64; 3]>,
    /// `Ns`
    pub shininess: Option<f64>,
    /// `Ni`
    pub optical_density: Option<f64>,
    /// `d`
    pub dissolve: Option<f64>,
    /// `Tr`: the transparency, 1 − `d`, as some writers spell it.
    pub transparency: Option<f64>,
    /// `illum`
    pub illumination: Option<u32>,
    /// `Pr`
    pub roughness: Option<f64>,
    /// `Pm`
    pub metallic: Option<f64>,
    /// `Ps`
    pub sheen: Option<f64>,
    /// `Pc`
    pub clearcoat: Option<f64>,
    /// `Pcr`
    pub clearcoat_roughness: Option<f64>,
    /// `aniso`
    pub anisotropy: Option<f64>,
    /// `anisor`
    pub anisotropy_rotation: Option<f64>,
    /// `map_Kd`, `bump`, `norm`, ... in file order.
    pub maps: Vec<TextureMap>,
    /// Other statements: keyword and the rest of the line, verbatim.
    pub other: Vec<(String, String)>,
}

/// A texture statement such as `map_Kd -s 2 2 1 wood.png`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextureMap {
    /// The statement keyword, as written (`map_Kd`, `map_Bump`, `bump`, `disp`, `refl`...).
    pub kind: String,
    /// The options before the file name.
    pub options: MapOptions,
    /// The file name, which may contain spaces, relative to the library.
    pub path: String,
}

/// A texture statement's options, each as the file gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapOptions {
    /// `-o u [v [w]]`: where the texture's coordinates move, missing values 0.
    pub offset: Option<[f64; 3]>,
    /// `-s u [v [w]]`: how they scale, missing values 1.
    pub scale: Option<[f64; 3]>,
    /// `-mm base gain`: the values read as `base + gain × value`.
    pub range: Option<[f64; 2]>,
    /// `-clamp on|off`: whether coordinates outside 0–1 read its edges.
    pub clamp: Option<bool>,
    /// `-imfchan r|g|b|m|l|z`: the channel a scalar map reads.
    pub channel: Option<char>,
    /// `-bm mult`: a bump map's multiplier.
    pub bump: Option<f64>,
    /// Other options with their values, verbatim, in file order
    /// (`-blendu off`, `-t 0.1 0.1`).
    pub other: Vec<(String, Vec<String>)>,
}

impl MaterialLibrary {
    /// The material named `name`.
    pub fn get(&self, name: &str) -> Option<&Material> {
        self.materials.iter().find(|material| material.name == name)
    }
}

impl Material {
    /// The first map of kind `kind` (compared without case).
    pub fn map(&self, kind: &str) -> Option<&TextureMap> {
        self.maps
            .iter()
            .find(|map| map.kind.eq_ignore_ascii_case(kind))
    }
}
