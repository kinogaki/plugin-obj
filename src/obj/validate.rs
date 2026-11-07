//! Consistency of an [`Obj`] built by hand: what the writer needs so that the
//! text it produces reads back as the same model.
use crate::obj::{Elements, NONE, Obj, Run};

impl Obj {
    /// Checks table lengths, element shapes, references, run positions,
    /// finite values and single-line names. The reader only produces valid
    /// models; the writer refuses invalid ones.
    pub fn validate(&self) -> Result<(), String> {
        let positions = self.positions.len();
        if !self.weights.is_empty() && self.weights.len() != positions {
            return Err("weights must be empty or one per position".into());
        }
        if !self.colors.is_empty() && self.colors.len() != positions {
            return Err("colors must be empty or one per position".into());
        }
        if self.weights.iter().any(|&w| w == 0.0 || !w.is_finite()) {
            return Err("weights must be finite and nonzero".into());
        }
        let finite = |table: &[[f64; 3]]| table.iter().flatten().all(|v| v.is_finite());
        if !finite(&self.positions)
            || !finite(&self.colors)
            || !finite(&self.texcoords)
            || !finite(&self.normals)
        {
            return Err("vertex values must be finite".into());
        }
        let tables = (positions, self.texcoords.len(), self.normals.len());
        elements(&self.faces, "faces", 3, tables)?;
        elements(&self.lines, "lines", 2, tables)?;
        elements(&self.points, "points", 1, tables)?;
        if !self.lines.normals.is_empty() {
            return Err("lines have no normals".into());
        }
        if !self.points.texcoords.is_empty() || !self.points.normals.is_empty() {
            return Err("point sets have positions only".into());
        }
        let counts = (self.faces.len(), self.lines.len(), self.points.len());
        runs(&self.objects, "objects", counts)?;
        runs(&self.groups, "groups", counts)?;
        runs(&self.materials, "materials", counts)?;
        runs(&self.smoothing, "smoothing", counts)?;
        // The writer interleaves statements of every kind with the elements,
        // so all runs together must be in one file order.
        let mut all: Vec<_> = self.objects.iter().map(start).collect();
        all.extend(self.groups.iter().map(start));
        all.extend(self.materials.iter().map(start));
        all.extend(self.smoothing.iter().map(start));
        all.sort_unstable();
        if all
            .windows(2)
            .any(|pair| pair[1].1 < pair[0].1 || pair[1].2 < pair[0].2)
        {
            return Err("runs of different kinds disagree on the element order".into());
        }
        let names = self
            .objects
            .iter()
            .chain(&self.groups)
            .chain(&self.materials);
        if let Some(name) = names
            .map(|run| run.value.as_str())
            .chain(self.material_libraries.iter().map(String::as_str))
            .find(|name| !readable_name(name))
        {
            return Err(format!(
                "name {name:?} would not read back: it must be one line without surrounding \
                 blanks or a word starting with #"
            ));
        }
        if self
            .groups
            .iter()
            .any(|run| run.value.split_whitespace().collect::<Vec<_>>().join(" ") != run.value)
        {
            return Err("group names must be separated by single spaces".into());
        }
        Ok(())
    }
}

fn elements(
    elements: &Elements,
    kind: &str,
    least: u32,
    (positions, texcoords, normals): (usize, usize, usize),
) -> Result<(), String> {
    let corners = elements.positions.len();
    let offsets = &elements.offsets;
    if offsets.first() != Some(&0) || *offsets.last().unwrap_or(&0) as usize != corners {
        return Err(format!(
            "{kind}: offsets must start at 0 and end at the corner count"
        ));
    }
    if offsets.windows(2).any(|pair| pair[1] < pair[0] + least) {
        return Err(format!("{kind}: each needs at least {least} corners"));
    }
    let check = |indices: &[u32], count: usize, table: &str, optional: bool| {
        if !indices.is_empty() && indices.len() != corners {
            return Err(format!(
                "{kind}: {table} references must be empty or one per corner"
            ));
        }
        if indices
            .iter()
            .any(|&i| i as usize >= count && !(optional && i == NONE))
        {
            return Err(format!("{kind}: a {table} reference is out of range"));
        }
        Ok(())
    };
    check(&elements.positions, positions, "position", false)?;
    check(&elements.texcoords, texcoords, "texcoord", true)?;
    check(&elements.normals, normals, "normal", true)
}

fn start<T>(run: &Run<T>) -> (u32, u32, u32) {
    (run.face, run.line, run.point)
}

fn runs<T>(
    runs: &[Run<T>],
    kind: &str,
    (faces, lines, points): (usize, usize, usize),
) -> Result<(), String> {
    let mut last = (0, 0, 0);
    for run in runs {
        let at = (run.face, run.line, run.point);
        if at.0 < last.0 || at.1 < last.1 || at.2 < last.2 {
            return Err(format!("{kind}: runs must be in file order"));
        }
        if run.face as usize > faces || run.line as usize > lines || run.point as usize > points {
            return Err(format!("{kind}: a run starts past the last element"));
        }
        last = at;
    }
    Ok(())
}

/// Whether the reader gives back `name` from `keyword name`: no line breaks,
/// no leading or trailing blanks, and no `#` starting a word (a comment).
fn readable_name(name: &str) -> bool {
    let blank = |c: char| matches!(c, ' ' | '\t' | '\r');
    !name.contains(['\n', '\r'])
        && !name.starts_with(blank)
        && !name.ends_with(blank)
        && !name.starts_with('#')
        && !name.contains(" #")
        && !name.contains("\t#")
}
