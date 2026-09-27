//! A minimal JSON reader for the test fixtures (no dependencies).

#[derive(Clone, Debug, PartialEq)]
pub enum J {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    pub fn get(&self, k: &str) -> &J {
        match self {
            J::Obj(v) => v.iter().find(|(a, _)| a == k).map(|(_, b)| b).unwrap_or_else(|| panic!("no key {k}")),
            _ => panic!("not an object"),
        }
    }
    pub fn arr(&self) -> &[J] {
        match self {
            J::Arr(v) => v,
            _ => panic!("not an array: {self:?}"),
        }
    }
    pub fn str(&self) -> &str {
        match self {
            J::Str(s) => s,
            _ => panic!("not a string: {self:?}"),
        }
    }
    pub fn b(&self) -> bool {
        match self {
            J::Bool(b) => *b,
            _ => panic!("not a bool: {self:?}"),
        }
    }
    pub fn int(&self) -> i64 {
        match self {
            J::Num(n) => *n as i64,
            _ => panic!("not a number: {self:?}"),
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, J::Null)
    }
    pub fn pairs(&self) -> &[(String, J)] {
        match self {
            J::Obj(v) => v,
            _ => panic!("not an object"),
        }
    }
}

pub fn parse(text: &str) -> J {
    let b: Vec<char> = text.chars().collect();
    let mut p = 0;
    let v = value(&b, &mut p);
    v
}

fn ws(b: &[char], p: &mut usize) {
    while *p < b.len() && b[*p].is_whitespace() {
        *p += 1;
    }
}

fn value(b: &[char], p: &mut usize) -> J {
    ws(b, p);
    match b[*p] {
        '{' => {
            *p += 1;
            let mut v = Vec::new();
            ws(b, p);
            if b[*p] == '}' {
                *p += 1;
                return J::Obj(v);
            }
            loop {
                ws(b, p);
                let k = match value(b, p) {
                    J::Str(s) => s,
                    _ => panic!("key"),
                };
                ws(b, p);
                assert_eq!(b[*p], ':');
                *p += 1;
                let x = value(b, p);
                v.push((k, x));
                ws(b, p);
                if b[*p] == ',' {
                    *p += 1;
                } else {
                    assert_eq!(b[*p], '}');
                    *p += 1;
                    return J::Obj(v);
                }
            }
        }
        '[' => {
            *p += 1;
            let mut v = Vec::new();
            ws(b, p);
            if b[*p] == ']' {
                *p += 1;
                return J::Arr(v);
            }
            loop {
                v.push(value(b, p));
                ws(b, p);
                if b[*p] == ',' {
                    *p += 1;
                } else {
                    assert_eq!(b[*p], ']');
                    *p += 1;
                    return J::Arr(v);
                }
            }
        }
        '"' => {
            *p += 1;
            let mut s = String::new();
            loop {
                let c = b[*p];
                *p += 1;
                match c {
                    '"' => return J::Str(s),
                    '\\' => {
                        let e = b[*p];
                        *p += 1;
                        match e {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            'r' => s.push('\r'),
                            'b' => s.push('\u{8}'),
                            'f' => s.push('\u{c}'),
                            'u' => {
                                let h: String = b[*p..*p + 4].iter().collect();
                                *p += 4;
                                let mut cp = u32::from_str_radix(&h, 16).unwrap();
                                if (0xD800..0xDC00).contains(&cp) {
                                    let h2: String = b[*p + 2..*p + 6].iter().collect();
                                    *p += 6;
                                    let lo = u32::from_str_radix(&h2, 16).unwrap();
                                    cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                                }
                                s.push(char::from_u32(cp).unwrap());
                            }
                            c => s.push(c),
                        }
                    }
                    c => s.push(c),
                }
            }
        }
        't' => {
            *p += 4;
            J::Bool(true)
        }
        'f' => {
            *p += 5;
            J::Bool(false)
        }
        'n' => {
            *p += 4;
            J::Null
        }
        _ => {
            let st = *p;
            while *p < b.len() && "+-0123456789.eE".contains(b[*p]) {
                *p += 1;
            }
            let t: String = b[st..*p].iter().collect();
            J::Num(t.parse().unwrap_or_else(|_| panic!("number {t}")))
        }
    }
}
