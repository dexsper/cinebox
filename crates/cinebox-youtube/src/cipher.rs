//! Signature and n-sig decipher from player JS (youtube-dl `_parse_sig_js`).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::Error;
use crate::jsinterp::{JSInterpreter, JsFunction, JsValue};

static PLAYER_JS_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""(?:PLAYER_JS_URL|jsUrl)"\s*:\s*"([^"]+)""#).expect("static regex")
});

static STS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:signatureTimestamp|sts)\s*:\s*(?P<sts>[0-9]{5})").expect("static regex")
});

static SIG_NAME: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    let pats = [
        r"\b(?P<var>[\w$]+)&&\((?P=var)=(?P<sig>[\w$]{2,})\(decodeURIComponent\((?P=var)\)\)",
        r#"(?P<sig>[\w$]+)\s*=\s*function\(\s*(?P<arg>[\w$]+)\s*\)\s*\{\s*(?P=arg)\s*=\s*(?P=arg)\.split\(\s*""\s*\)\s*;\s*[^}]+;\s*return\s+(?P=arg)\.join\(\s*""\s*\)"#,
        r#"(?:\b|[^\w$])(?P<sig>[\w$]{2,})\s*=\s*function\(\s*a\s*\)\s*\{\s*a\s*=\s*a\.split\(\s*""\s*\)(?:;[\w$]{2}\.[\w$]{2}\(a,\d+\))?"#,
        r"\b[cs]\s*&&\s*[adf]\.set\([^,]+\s*,\s*encodeURIComponent\s*\(\s*(?P<sig>[\w$]+)\(",
        r"\b[\w]+\s*&&\s*[\w]+\.set\([^,]+\s*,\s*encodeURIComponent\s*\(\s*(?P<sig>[\w$]+)\(",
        r"\bm=(?P<sig>[\w$]{2,})\(decodeURIComponent\(h\.s\)\)",
        r#"("|')signature\1\s*,\s*(?P<sig>[\w$]+)\("#,
        r"\.sig\|\|(?P<sig>[\w$]+)\(",
        r"\b[cs]\s*&&\s*[adf]\.set\([^,]+\s*,\s*(?P<sig>[\w$]+)\(",
        r"\bc\s*&&\s*[\w]+\.set\([^,]+\s*,\s*\([^)]*\)\s*\(\s*(?P<sig>[\w$]+)\(",
    ];
    let mut out = Vec::with_capacity(pats.len());

    for pat in pats {
        if let Ok(re) = Regex::new(pat) {
            out.push(re);
        }
    }

    out
});

static NSIG_ASSIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([A-Za-z_$][\w$]*)\s*=\s*function\(([A-Za-z_$][\w$]*)\)\s*\{").expect("static regex")
});

static NSIG_ARRAY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\n;]var\s+[A-Za-z_$][\w$]*\s*=\s*\[(?P<nfunc>[A-Za-z_$][\w$]*)\]\s*[;\n]")
        .expect("static regex")
});

pub(crate) fn player_js_url(html: &str) -> Option<&str> {
    PLAYER_JS_URL
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

pub(crate) fn signature_timestamp(js: &str) -> Option<u32> {
    let caps = STS_RE.captures(js)?;
    let sts = caps.name("sts")?.as_str();

    sts.parse().ok()
}

pub(crate) struct Decipher {
    interp: JSInterpreter,
    sig: Option<JsFunction>,
    nsig: Option<JsFunction>,
    perm: Option<(usize, Vec<usize>)>,
    n_map: HashMap<String, String>,
}

impl Decipher {
    pub(crate) fn new(js: String) -> Self {
        Self {
            interp: JSInterpreter::new(js),
            sig: None,
            nsig: None,
            perm: None,
            n_map: HashMap::new(),
        }
    }

    pub(crate) fn decrypt_sig(&mut self, s: &str) -> Result<String, Error> {
        if let Some((len, perm)) = self.perm.as_ref() {
            if *len == s.chars().count() {
                return apply_perm(s, perm);
            }
        }

        let func = self.sig_fn()?;
        let test = test_string(s.chars().count());
        let permuted = call_str(&mut self.interp, &func, &test, None)?;
        let perm = perm_from_test(&permuted, s.chars().count())?;

        let out = apply_perm(s, &perm)?;
        self.perm = Some((s.chars().count(), perm));

        Ok(out)
    }

    pub(crate) fn decrypt_n(&mut self, n: &str) -> Result<String, Error> {
        if let Some(cached) = self.n_map.get(n) {
            return Ok(cached.clone());
        }

        let func = self.nsig_fn()?;
        let mut kwargs = HashMap::with_capacity(1);
        kwargs.insert(
            String::from("_ytdl_do_not_return"),
            JsValue::from_str(n),
        );
        let out = call_str(&mut self.interp, &func, n, Some(&kwargs))?;
        let enhanced = out.starts_with("enhanced_except_");
        let unchanged = out.ends_with(n);

        if enhanced || unchanged {
            return Err(Error::BadNsig);
        }

        self.n_map.insert(n.to_owned(), out.clone());

        Ok(out)
    }

    fn sig_fn(&mut self) -> Result<JsFunction, Error> {
        if let Some(func) = self.sig.clone() {
            return Ok(func);
        }

        let name = find_sig_name(self.interp_code()).ok_or(Error::BadSig)?;
        let func = extract_named(&mut self.interp, &name)?;
        self.sig = Some(func.clone());

        Ok(func)
    }

    fn nsig_fn(&mut self) -> Result<JsFunction, Error> {
        if let Some(func) = self.nsig.clone() {
            return Ok(func);
        }

        let name = find_n_name(self.interp_code()).ok_or(Error::BadNsig)?;
        let func = extract_named(&mut self.interp, &name)?;
        self.nsig = Some(func.clone());

        Ok(func)
    }

    fn interp_code(&self) -> &str {
        self.interp.code_str()
    }
}

fn extract_named(interp: &mut JSInterpreter, name: &str) -> Result<JsFunction, Error> {
    let (args, body) = interp.extract_function_code(name)?;
    let func = interp.extract_with_body(name, args, body)?;

    Ok(func)
}

fn find_sig_name(js: &str) -> Option<String> {
    for re in SIG_NAME.iter() {
        let Some(caps) = re.captures(js) else {
            continue;
        };

        if let Some(name) = caps.name("sig") {
            return Some(name.as_str().to_owned());
        }
    }

    None
}

fn find_n_name(js: &str) -> Option<String> {
    for caps in NSIG_ASSIGN.captures_iter(js) {
        let Some(name) = caps.get(1) else {
            continue;
        };

        let Some(m) = caps.get(0) else {
            continue;
        };

        let rest = js.get(m.end()..).unwrap_or("");
        let body_end = rest.find("};").unwrap_or(rest.len().min(8000));
        let body = rest.get(..body_end).unwrap_or(rest);
        let has_except = body.contains("enhanced_except_");
        let has_w8 = body.contains("_w8_");

        if has_except || has_w8 {
            return Some(name.as_str().to_owned());
        }
    }

    let Some(caps) = NSIG_ARRAY.captures(js) else {
        return None;
    };

    caps.name("nfunc").map(|m| m.as_str().to_owned())
}

fn test_string(len: usize) -> String {
    let mut s = String::with_capacity(len);

    for i in 0..len {
        if let Some(c) = char::from_u32(i as u32) {
            s.push(c);
        }
    }

    s
}

fn perm_from_test(permuted: &str, len: usize) -> Result<Vec<usize>, Error> {
    let mut perm = Vec::with_capacity(len);

    for c in permuted.chars() {
        perm.push(c as usize);
    }

    if perm.len() != len {
        return Err(Error::BadSig);
    }

    Ok(perm)
}

fn apply_perm(s: &str, perm: &[usize]) -> Result<String, Error> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());

    for &i in perm {
        let Some(c) = chars.get(i) else {
            return Err(Error::BadSig);
        };

        out.push(*c);
    }

    Ok(out)
}

fn call_str(
    interp: &mut JSInterpreter,
    func: &JsFunction,
    arg: &str,
    kwargs: Option<&HashMap<String, JsValue>>,
) -> Result<String, Error> {
    let args = [JsValue::from_str(arg)];
    let val = interp.invoke(func, &args, kwargs)?;

    Ok(val.to_js_string())
}
