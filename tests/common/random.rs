//! Seeded random OBJ text and the generator behind it.

/// A small deterministic generator (xorshift64*).
pub struct Random(u64);

impl Random {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// A random number spelling: plain, exponent, long, signed, odd forms. Positions
/// stay within luce-geocore's ±1e6 bound.
fn number(random: &mut Random) -> String {
    let value = (random.unit() * 2.0 - 1.0) * 10f64.powi(random.below(12) as i32 - 6);
    match random.below(10) {
        0 => format!("{value}"),
        1 => format!("{value:.6}"),
        2 => format!("{value:e}"),
        3 => format!("{value:.17e}"),
        4 => format!("{value:.25}"),
        5 => format!("{}", random.below(2000) as i64 - 1000),
        6 => [
            "+1.5", ".5", "5.", "-0", "-0.0", "0.000", "1e0", "007.25", "-.125e+3", "1E-3",
        ][random.below(10) as usize]
            .to_string(),
        7 => format!("{:.3}", value * 3.0),
        8 => format!("{value:.9E}"),
        _ => format!("{:.20}", value * 1e-12),
    }
}

/// Random valid OBJ text: vertices with weights, vt with one to three values,
/// faces in every reference form (negative ones included), statements,
/// comments, blank lines, CRLF and tab runs.
pub fn random_obj(seed: u64, faces: usize) -> Vec<u8> {
    let mut random = Random::new(seed);
    let mut text = String::from("# random\nmtllib a.mtl\n");
    let (mut v, mut vt, mut vn) = (0i64, 0i64, 0i64);
    let blank =
        |random: &mut Random| [" ", " ", "\t", "  ", " \t "][random.below(5) as usize].to_string();
    let end = |random: &mut Random| {
        ["\n", "\n", "\n", "\r\n", " # note\n"][random.below(5) as usize].to_string()
    };
    for _ in 0..faces {
        for _ in 0..random.below(4) + 1 {
            let mut line = format!(
                "v{}{}{}{}{}{}",
                blank(&mut random),
                number(&mut random),
                blank(&mut random),
                number(&mut random),
                blank(&mut random),
                number(&mut random)
            );
            if random.below(8) == 0 {
                line += &format!(" {}", 1.0 + random.below(4) as f64 * 0.5);
            }
            text += &(line + &end(&mut random));
            v += 1;
        }
        if random.below(3) == 0 {
            let values = random.below(3) + 1;
            let parts: Vec<String> = (0..values).map(|_| number(&mut random)).collect();
            // No comment here: luce-obj reads the word after `# ` as a third value.
            text += &format!("vt {}\n", parts.join(" "));
            vt += 1;
        }
        if random.below(4) == 0 {
            text += &format!(
                "vn {} {} {}{}",
                number(&mut random),
                number(&mut random),
                number(&mut random),
                end(&mut random)
            );
            vn += 1;
        }
        match random.below(30) {
            0 => text += "o piece\n",
            1 => text += "g a b\n",
            2 => text += "usemtl m\n",
            3 => text += "s 1\n",
            4 => text += "\n# comment line\n\n",
            _ => {}
        }
        if v < 3 {
            continue;
        }
        // Distinct points per face: luce-geocore rejects collapsed edges.
        let corners = (3 + random.below(3)).min(v as u64) as usize;
        let mut points: Vec<i64> = Vec::new();
        while points.len() < corners {
            let index = random.below(v as u64) as i64;
            if !points.contains(&index) {
                points.push(index);
            }
        }
        let spell = |random: &mut Random, index: i64, count: i64| {
            if random.below(3) == 0 {
                (index - count).to_string()
            } else {
                (index + 1).to_string()
            }
        };
        let form = random.below(4);
        let mut line = String::from("f");
        for &index in &points {
            let p = spell(&mut random, index, v);
            let other = |random: &mut Random, count: i64| {
                let index = random.below(count as u64) as i64;
                spell(random, index, count)
            };
            let corner = match form {
                1 if vt > 0 => format!("{p}/{}", other(&mut random, vt)),
                2 if vn > 0 => format!("{p}//{}", other(&mut random, vn)),
                3 if vt > 0 && vn > 0 => {
                    let t = other(&mut random, vt);
                    format!("{p}/{t}/{}", other(&mut random, vn))
                }
                _ => p,
            };
            line += &blank(&mut random);
            line += &corner;
        }
        text += &(line + &end(&mut random));
    }
    text.into_bytes()
}
