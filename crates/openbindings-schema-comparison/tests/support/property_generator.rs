use serde_json::{Value, json};

/// SplitMix64: stable across platforms and Rust/rand versions.
#[derive(Debug)]
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    pub fn n(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    pub fn bit(&mut self) -> bool {
        self.n(2) == 0
    }
    pub fn small(&mut self) -> i64 {
        self.n(13) as i64 - 6
    }
}

pub fn value(r: &mut Rng, depth: usize) -> Value {
    let choices = if depth == 0 { 6 } else { 8 };
    match r.n(choices) {
        0 => Value::Null,
        1 => json!(r.bit()),
        2 => json!(r.small()),
        3 => json!(r.small() as f64 + 0.5),
        4 => json!(["", "a", "ab", "é", "😀", "a😀", "aaaaaa"][r.n(7)]),
        5 => json!(r.n(1000)),
        6 => {
            let n = r.n(4);
            Value::Array((0..n).map(|_| value(r, depth - 1)).collect())
        }
        _ => {
            let mut m = serde_json::Map::new();
            for name in ["a", "b", "c"] {
                if r.bit() {
                    m.insert(name.into(), value(r, depth - 1));
                }
            }
            Value::Object(m)
        }
    }
}

pub fn schema(r: &mut Rng, depth: usize) -> Value {
    let k = r.n(if depth == 0 { 7 } else { 13 });
    match k {
        0 => json!(r.bit()),
        1 => json!({}),
        2 => {
            let a = r.small();
            let b = a + r.n(7) as i64;
            let mut s = json!({"type":if r.bit(){"number"}else{"integer"}});
            s[if r.bit() {
                "minimum"
            } else {
                "exclusiveMinimum"
            }] = json!(a);
            s[if r.bit() {
                "maximum"
            } else {
                "exclusiveMaximum"
            }] = json!(b);
            s
        }
        3 => {
            let a = r.n(4);
            json!({"type":"string","minLength":a,"maxLength":a+r.n(5)})
        }
        4 => {
            let a = value(r, 0);
            let b = value(r, 0);
            if a == b {
                json!({"enum":[a]})
            } else {
                json!({"enum":[a,b]})
            }
        }
        5 => json!({"const":value(r,1)}),
        6 => {
            json!({"type":(["null","boolean","string","integer","number","array","object"][r.n(7)])})
        }
        7 | 8 => {
            let mut p = serde_json::Map::new();
            let mut req = vec![];
            for n in ["a", "b", "c"] {
                if r.bit() {
                    p.insert(n.into(), schema(r, depth - 1));
                    if r.bit() {
                        req.push(n);
                    }
                }
            }
            let mut s = json!({"type":"object","properties":p,"required":req});
            match r.n(4) {
                0 => s["additionalProperties"] = json!(false),
                1 => s["additionalProperties"] = schema(r, depth - 1),
                _ => {}
            }
            s
        }
        9 => {
            let a = r.n(3);
            json!({"type":"array","items":schema(r,depth-1),"minItems":a,"maxItems":a+r.n(4)})
        }
        10 | 11 => {
            let a = schema(r, depth - 1);
            let b = schema(r, depth - 1);
            let key = if k == 10 { "anyOf" } else { "oneOf" };
            json!({key:[a,b]})
        }
        _ => json!({"allOf":[schema(r,depth-1),{"description":"annotation"}]}),
    }
}

// Only schema-position arrays are permuted; const/enum instance objects retain meaning.
pub fn permute(v: &Value, which: &str) -> Value {
    let mut s = v.clone();
    let Some(m) = s.as_object_mut() else { return s };
    for (key, v) in m.iter_mut() {
        if key == which
            && let Some(a) = v.as_array_mut()
        {
            a.reverse();
        }
        match key.as_str() {
            "properties" | "$defs" => {
                if let Some(m) = v.as_object_mut() {
                    for child in m.values_mut() {
                        *child = permute(child, which);
                    }
                }
            }
            "items" | "additionalProperties" | "not" => *v = permute(v, which),
            "anyOf" | "oneOf" | "allOf" => {
                if let Some(a) = v.as_array_mut() {
                    for child in a {
                        *child = permute(child, which);
                    }
                }
            }
            _ => {}
        }
    }
    s
}
