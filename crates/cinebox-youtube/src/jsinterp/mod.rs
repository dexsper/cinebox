//! JS interpreter ported from youtube-dl `jsinterp.py`.

mod date;
mod separate;
mod value;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use regex::Regex;

pub use value::{JsFunction, JsRegex, JsValue};

use crate::error::JsError;
use separate::{comma_split, separate, separate_at_paren, ALL_OPS, OP_CHARS};
use value::{
    js_add, js_cmp, js_div, js_eq, js_exp, js_mod, js_mul, js_number_string, js_shift_count,
    js_strict_eq, js_sub, js_to_int32, CmpOp, EnvMap, EnvRc, EnvSlot, RE_G, RE_I, RE_M, RE_S,
    RE_U,
};

const NAME: &str = r"[A-Za-z_$][0-9A-Za-z_$]*";
const OBJ_PREFIX: &str = "__youtube_dl_jsinterp_obj";

static COMPOUND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<try>try)\s*\{|(?P<if>if)\s*\(|(?P<switch>switch)\s*\(|(?P<for>for)\s*\(|(?P<while>while)\s*\(")
        .expect("static regex")
});

static FINALLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"finally\s*\{").expect("static regex")
});

static CATCH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"catch\s*(?:\((?P<err>\s*{NAME}\s*)\))?\s*\{{"
    ))
    .expect("static regex")
});

static NESTED_FN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"function\((?P<args>[^)]*)\)\s*\{").expect("static regex")
});

static OBJECT_FN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?P<key>{NAME})\s*:\s*function\s*\((?P<args>(?:{NAME}|,)*)\)\{{(?P<code>[^}}]+)\}}"
    ))
    .expect("static regex")
});

static INC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"(?P<pre_sign>\+\+|--)(?P<var1>{NAME})|(?P<var2>{NAME})(?P<post_sign>\+\+|--)"
    ))
    .expect("static regex")
});

static NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^{NAME}$")).expect("static regex")
});

/// youtube-dl `JSInterpreter`.
pub struct JSInterpreter {
    code: Rc<str>,
    functions: HashMap<String, JsFunction>,
    objects: HashMap<String, JsValue>,
    scratch: HashMap<String, JsValue>,
    named_counter: u32,
}

struct LocalNs {
    maps: Vec<EnvRc>,
}

impl LocalNs {
    fn new(maps: Vec<EnvRc>) -> Self {
        if maps.is_empty() {
            return Self {
                maps: vec![Rc::new(RefCell::new(HashMap::new()))],
            };
        }

        Self { maps }
    }

    fn get(&self, key: &str) -> JsValue {
        for map in &self.maps {
            if let Some(val) = map.borrow().get(key) {
                return val.to_value();
            }
        }

        JsValue::Undefined
    }

    fn set(&self, key: &str, val: JsValue) {
        let val = EnvSlot::from_value(val);

        for map in &self.maps {
            if map.borrow().contains_key(key) {
                map.borrow_mut().insert(key.to_owned(), val);
                return;
            }
        }

        self.maps[0].borrow_mut().insert(key.to_owned(), val);
    }
}

impl JSInterpreter {
    #[must_use]
    pub fn new(code: impl Into<String>) -> Self {
        Self::new_rc(Rc::<str>::from(code.into()))
    }

    fn new_rc(code: Rc<str>) -> Self {
        Self {
            code,
            functions: HashMap::new(),
            objects: HashMap::new(),
            scratch: HashMap::new(),
            named_counter: 0,
        }
    }

    pub(crate) fn code_str(&self) -> &str {
        &self.code
    }

    /// Call a top-level function by name.
    ///
    /// # Errors
    ///
    /// Missing function or interpreter failure.
    pub fn call_function(&mut self, name: &str, args: &[JsValue]) -> Result<JsValue, JsError> {
        let func = self.extract_function(name)?;

        let result = self.run_function(&func, args, None, 100);
        self.scratch.clear();
        result
    }

    pub(crate) fn call_extracted(
        func: &JsFunction,
        args: &[JsValue],
        kwargs: Option<&std::collections::HashMap<String, JsValue>>,
        rec: i32,
    ) -> Result<JsValue, JsError> {
        let mut interp = Self::new_rc(func.source());
        interp.run_function(func, args, kwargs, rec)
    }

    /// Extract a named function (`F<name>`).
    ///
    /// # Errors
    ///
    /// Function not found.
    pub fn extract_function(&mut self, name: &str) -> Result<JsFunction, JsError> {
        self.extract_function_with(name, Vec::new())
    }

    /// Extract with extra global stacks (youtube-dl `extract_function(name, *global_stack)`).
    ///
    /// # Errors
    ///
    /// Function not found.
    pub fn extract_function_with(
        &mut self,
        name: &str,
        extra: Vec<std::collections::HashMap<String, JsValue>>,
    ) -> Result<JsFunction, JsError> {
        let (args, body) = self.extract_function_code(name)?;

        let mut stack = Vec::with_capacity(extra.len());

        for m in extra {
            let mut slots = EnvMap::new();

            for (k, v) in m {
                slots.insert(k, EnvSlot::from_value(v));
            }

            stack.push(Rc::new(RefCell::new(slots)));
        }

        self.extract_function_from_code(name, args, body, &mut stack)
    }

    pub(crate) fn extract_function_code(&self, name: &str) -> Result<(Vec<String>, String), JsError> {
        let escaped = regex::escape(name);
        let pat = format!(
            r"(?s)(?:function\s+{escaped}|[{{;,]\s*{escaped}\s*=\s*function|(?:var|const|let)\s+{escaped}\s*=\s*function)\s*\((?P<args>[^)]*)\)\s*(?P<code>\{{.+}})"
        );
        let re = Regex::new(&pat).map_err(|err| JsError::msg(err.to_string()))?;
        let missing = format!("could not find JS function \"{name}\"");
        let Some(caps) = re.captures(&self.code) else {
            return Err(JsError::msg(missing));
        };

        let args_txt = caps.name("args").map(|m| m.as_str()).unwrap_or("");
        let args = build_arglist(args_txt);
        let code = caps.name("code").map(|m| m.as_str()).unwrap_or("");
        let (body, _) = separate_at_paren(code, Some('}'))?;

        Ok((args, body))
    }

    pub(crate) fn extract_with_body(
        &mut self,
        name: &str,
        argnames: Vec<String>,
        code: String,
    ) -> Result<JsFunction, JsError> {
        let mut stack = Vec::new();

        self.extract_function_from_code(name, argnames, code, &mut stack)
    }

    fn extract_function_from_code(
        &mut self,
        name: &str,
        argnames: Vec<String>,
        mut code: String,
        stack: &mut Vec<EnvRc>,
    ) -> Result<JsFunction, JsError> {
        let local = Rc::new(RefCell::new(HashMap::new()));
        let mut nested = HashMap::new();

        while let Some(m) = NESTED_FN.find(&code) {
            let start = m.start();
            let nested_src = &code[start..];
            let Some(caps) = NESTED_FN.captures(nested_src) else {
                return Err(JsError::msg("nested function"));
            };

            let args_txt = caps.name("args").map(|m| m.as_str()).unwrap_or("");
            let body_start = start + caps.get(0).map(|m| m.end()).unwrap_or(0) - 1;
            let (body, remaining) = separate_at_paren(&code[body_start..], Some('}'))?;
            let nested_args = build_arglist(args_txt);
            let mut child_stack = Vec::with_capacity(stack.len() + 1);

            child_stack.push(Rc::clone(&local));
            child_stack.extend(stack.iter().cloned());

            let n = self.next_named_id();
            let fname = u32_string(n);
            let nested_fn = self.extract_function_from_code(&fname, nested_args, body, &mut child_stack)?;
            let obj_name = named_var(n);

            nested.insert(obj_name.clone(), nested_fn);
            code = concat3(&code[..start], &obj_name, &remaining);
        }

        let mut closure = Vec::with_capacity(stack.len() + 1);
        closure.push(local);
        closure.extend(stack.iter().cloned());
        let source = Rc::clone(&self.code);

        Ok(JsFunction::new(name, argnames, code, source, closure, nested))
    }

    fn run_function(
        &mut self,
        func: &JsFunction,
        args: &[JsValue],
        kwargs: Option<&std::collections::HashMap<String, JsValue>>,
        rec: i32,
    ) -> Result<JsValue, JsError> {
        self.push_nested(func);

        let mut maps: Vec<EnvRc> = func.closure().to_vec();

        if maps.is_empty() {
            maps.push(Rc::new(RefCell::new(HashMap::new())));
        }

        {
            let mut first = maps[0].borrow_mut();

            for (i, name) in func.argnames().iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                first.insert(name.clone(), EnvSlot::from_value(val));
            }

            if let Some(kwargs) = kwargs {
                for (key, val) in kwargs {
                    first.insert(key.clone(), EnvSlot::from_value(val.clone()));
                }
            }
        }

        let local = LocalNs::new(maps);
        let body = func.body().replace('\n', " ");
        let (ret, abort) = self.interpret_statement(&body, &local, rec - 1)?;

        if abort {
            return Ok(ret);
        }

        Ok(JsValue::Null)
    }

    pub(crate) fn invoke(
        &mut self,
        func: &JsFunction,
        args: &[JsValue],
        kwargs: Option<&HashMap<String, JsValue>>,
    ) -> Result<JsValue, JsError> {
        self.run_function(func, args, kwargs, 100)
    }

    fn push_nested(&mut self, func: &JsFunction) {
        for (name, nested) in func.nested() {
            if self.scratch.contains_key(name) {
                continue;
            }

            let func_val = JsValue::Function(nested.clone());
            self.scratch.insert(name.clone(), func_val);
            self.push_nested(nested);
        }
    }

    fn next_named_id(&mut self) -> u32 {
        self.named_counter = self.named_counter.saturating_add(1);
        self.named_counter
    }

    fn named_object(&mut self, obj: JsValue) -> String {
        let n = self.next_named_id();
        let name = named_var(n);
        self.scratch.insert(name.clone(), obj);
        name
    }

    fn lookup(&self, local: &LocalNs, name: &str) -> JsValue {
        let val = local.get(name);

        if !matches!(val, JsValue::Undefined) {
            return val;
        }

        if let Some(val) = self.scratch.get(name) {
            return val.clone();
        }

        if let Some(func) = self.functions.get(name) {
            return JsValue::Function(func.clone());
        }

        if let Some(obj) = self.objects.get(name) {
            return obj.clone();
        }

        JsValue::Undefined
    }

    fn dump(&mut self, obj: &JsValue) -> String {
        match obj {
            JsValue::Undefined => String::from("undefined"),
            JsValue::Null => String::from("null"),
            JsValue::Bool(true) => String::from("true"),
            JsValue::Bool(false) => String::from("false"),
            JsValue::Number(n) if n.is_nan() => String::from("NaN"),
            JsValue::Number(n) if n.is_infinite() && n.is_sign_positive() => {
                String::from("Infinity")
            }
            JsValue::Number(n) if n.is_infinite() => String::from("-Infinity"),
            JsValue::Number(n) => js_number_string(*n),
            JsValue::String(s) => serde_json::to_string(s.as_ref()).unwrap_or_else(|_| {
                let mut out = String::with_capacity(s.len() + 2);
                out.push('"');
                out.push_str(s);
                out.push('"');
                out
            }),
            other => self.named_object(other.clone()),
        }
    }

    fn continue_with(
        &mut self,
        val: JsValue,
        remaining: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<(JsValue, bool), JsError> {
        if remaining.trim().is_empty() {
            return Ok((val, should_return));
        }

        let name = self.named_object(val);
        let next = concat2(&name, remaining);
        let (ret, abort) = self.interpret_statement(&next, local, rec)?;

        Ok((ret, abort || should_return))
    }

    fn interpret_expression(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
    ) -> Result<JsValue, JsError> {
        let (ret, should_return) = self.interpret_statement(expr, local, rec)?;

        if should_return {
            return Err(JsError::msg("cannot return from an expression"));
        }

        Ok(ret)
    }

    fn interpret_iter(
        &mut self,
        list_txt: &str,
        local: &LocalNs,
        rec: i32,
    ) -> Result<Vec<JsValue>, JsError> {
        let parts = comma_split(list_txt);
        let only_empty = parts.len() == 1;
        let mut out = Vec::with_capacity(parts.len());

        for part in parts {
            if part.trim().is_empty() && only_empty {
                continue;
            }

            out.push(self.interpret_expression(&part, local, rec)?);
        }

        Ok(out)
    }

    fn interpret_statement(
        &mut self,
        stmt: &str,
        local: &LocalNs,
        rec: i32,
    ) -> Result<(JsValue, bool), JsError> {
        if rec < 0 {
            return Err(JsError::Recursion);
        }

        let rec = rec - 1;
        let mut should_return = false;
        let mut parts = separate(stmt, ";", None, &[]);
        if parts.is_empty() {
            parts.push(String::new());
        }

        let expr_raw = parts.pop().map(|s| s.trim().to_owned()).unwrap_or_default();
        for sub in &parts {
            let (ret, abort) = self.interpret_statement(sub, local, rec)?;

            if abort && blocked_return(local, &ret) {
                continue;
            }

            if abort {
                return Ok((ret, true));
            }
        }

        let mut expr = expr_raw;
        if let Some(prefix) = stmt_prefix(&expr) {
            expr = expr[prefix.end..].trim().to_owned();
            if prefix.throw {
                let val = self.interpret_expression(&expr, local, rec)?;
                return Err(JsError::Throw(val.to_js_string()));
            }
            should_return = prefix.ret;
        }

        if expr.is_empty() {
            return Ok((JsValue::Null, should_return));
        }

        if let Some(result) = self.try_quote_prefix(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        if let Some(result) = self.try_new_kw(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        if let Some(result) = self.try_unary(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        if expr.starts_with('{') {
            let (inner, outer) = separate_at_paren(&expr, Some('}'))?;
            let chunks = comma_split(&inner);
            let mut sub_exprs = Vec::with_capacity(chunks.len());

            for sub in chunks {
                let parts = separate(sub.trim(), ":", Some(1), &[]);
                let parts: Vec<String> = parts.into_iter().map(|s| s.trim().to_owned()).collect();
                sub_exprs.push(parts);
            }

            let inner_empty = inner.trim().is_empty();
            let all_pairs = sub_exprs.iter().all(|p| p.len() == 2);
            let is_object = inner_empty || (!sub_exprs.is_empty() && all_pairs);

            if inner_empty {
                let val = JsValue::object(HashMap::new());

                if outer.is_empty() {
                    return Ok((val, should_return));
                }

                expr = concat2(&self.dump(&val), &outer);
            }

            if !inner_empty && is_object {
                let mut map = HashMap::with_capacity(sub_exprs.len());

                for pair in &sub_exprs {
                    let key = object_key(self, &pair[0], local, rec)?;
                    let val = self.interpret_expression(&pair[1], local, rec)?;
                    map.insert(key, val);
                }

                let val = JsValue::object(map);

                if outer.is_empty() {
                    return Ok((val, should_return));
                }

                expr = concat2(&self.dump(&val), &outer);
            }

            if !inner_empty && !is_object {
                let (inner_val, abort) = self.interpret_statement(&inner, local, rec)?;
                let done = outer.is_empty() || abort;

                if done {
                    return Ok((inner_val, abort || should_return));
                }

                expr = concat2(&self.dump(&inner_val), &outer);
            }
        }

        if expr.starts_with('(') {
            let (inner, outer) = separate_at_paren(&expr, Some(')'))?;
            let (inner_val, abort) = self.interpret_statement(&inner, local, rec)?;

            if outer.is_empty() || abort {
                return Ok((inner_val, abort || should_return));
            }

            expr = concat2(&self.dump(&inner_val), &outer);
        }

        if expr.starts_with('[') {
            let (inner, outer) = separate_at_paren(&expr, Some(']'))?;
            let items = self.interpret_iter(&inner, local, rec)?;
            let name = self.named_object(JsValue::array(items));
            expr = concat2(&name, &outer);
        }

        if let Some(result) = self.try_compound(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        let sub_exprs = comma_split(&expr);
        if sub_exprs.len() > 1 {
            let mut ret = JsValue::Null;
            for sub in &sub_exprs {
                let (v, abort) = self.interpret_statement(sub, local, rec)?;
                ret = v;
                if abort {
                    return Ok((ret, true));
                }
            }
            return Ok((ret, false));
        }

        expr = self.apply_increments(&expr, local);

        if expr.is_empty() {
            return Ok((JsValue::Null, should_return));
        }

        if expr == "break" {
            return Err(JsError::Break);
        }
        if expr == "continue" {
            return Err(JsError::Continue);
        }
        if expr == "undefined" {
            return Ok((JsValue::Undefined, should_return));
        }
        if expr == "NaN" {
            return Ok((JsValue::nan(), should_return));
        }
        if expr == "Infinity" {
            return Ok((JsValue::infinity(), should_return));
        }
        if expr.bytes().all(|b| b.is_ascii_digit()) {
            let n: f64 = expr.parse().unwrap_or(0.0);
            return Ok((JsValue::Number(n), should_return));
        }

        if let Some(result) = self.try_assign(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        if let Some(result) = self.try_indexing(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        if let Some(op_split) = self.separate_at_op(&expr) {
            let (op, left, right) = op_split;
            let val = self.eval_operator(&op, &left, &right, local, rec)?;
            return Ok((val, should_return));
        }

        if let Some(result) = self.try_member(&expr, local, rec, should_return)? {
            return Ok(result);
        }

        if let Some((fname, args_txt)) = split_call(&expr) {
            let argvals = self.interpret_iter(&args_txt, local, rec)?;
            let got = self.lookup(local, fname);

            if let JsValue::Function(func) = got {
                let val = self.run_function(&func, &argvals, None, rec)?;
                return Ok((val, should_return));
            }

            if matches!(got, JsValue::Undefined) {
                if !self.functions.contains_key(fname) {
                    let func = self.extract_function(fname)?;
                    self.functions.insert(fname.to_owned(), func);
                }

                if let Some(func) = self.functions.get(fname).cloned() {
                    let val = self.run_function(&func, &argvals, None, rec)?;
                    return Ok((val, should_return));
                }
            }
        }

        if let Some(val) = try_literal(&expr) {
            return Ok((val, should_return));
        }

        if NAME_RE.is_match(&expr) {
            let val = self.lookup(local, &expr);

            if should_return && blocked_return(local, &val) {
                return Ok((val, false));
            }

            return Ok((val, should_return));
        }

        let snippet: String = expr.chars().take(40).collect();

        Err(JsError::msg(format!("unsupported JS expression {snippet}")))
    }

    fn try_quote_prefix(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        let Some(q) = expr.chars().next() else {
            return Ok(None);
        };

        if q != '\'' && q != '"' && q != '/' {
            return Ok(None);
        }

        let mut parts = separate(expr, &q.to_string(), Some(1), &[]);
        if parts.len() < 2 {
            return Ok(None);
        }

        let inner = std::mem::take(&mut parts[0]);
        let mut outer = parts[1].clone();

        if q == '/' {
            let (flags, rest) = regex_flags(&outer);
            outer = rest;
            let pattern: String = inner.chars().skip(1).collect();
            let re = JsRegex::new(&pattern, flags)?;
            let val = JsValue::Regex(Rc::new(re));
            let result = self.continue_with(val, &outer, local, rec, should_return)?;

            return Ok(Some(result));
        }

        let quoted = concat2(&inner, &q.to_string());
        let Some(val) = parse_js_string(&quoted) else {
            return Ok(None);
        };

        let result = self.continue_with(val, &outer, local, rec, should_return)?;

        Ok(Some(result))
    }

    fn try_new_kw(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        let Some(obj) = expr.strip_prefix("new ") else {
            return Ok(None);
        };

        for klass in ["Date", "RegExp", "Error"] {
            let Some(after) = obj.strip_prefix(klass) else {
                continue;
            };

            if !after.starts_with('(') {
                continue;
            }

            let (left, right) = separate_at_paren(after, Some(')'))?;
            let argvals = self.interpret_iter(&left, local, rec)?;
            let val = match klass {
                "Date" => JsValue::Number(date::construct(&argvals)),
                "RegExp" => {
                    let pat = argvals.first().map(JsValue::to_js_string).unwrap_or_default();
                    let flags = argvals.get(1).map(flags_from_value).unwrap_or(0);
                    JsValue::Regex(Rc::new(JsRegex::new(&pat, flags)?))
                }
                _ => JsValue::from_str("Error"),
            };

            let result = self.continue_with(val, &right, local, rec, should_return)?;

            return Ok(Some(result));
        }

        Err(JsError::msg("unsupported object"))
    }

    fn try_unary(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        for op in ["void", "typeof", "!"] {
            let Some(operand) = expr.strip_prefix(op) else {
                continue;
            };

            if operand.is_empty() {
                continue;
            }

            let needs_space = op.chars().all(|c| c.is_ascii_alphabetic());

            if needs_space && !operand.starts_with(' ') {
                continue;
            }

            let operand = operand.to_owned();
            let val = self.eval_operator(op, &operand, "", local, rec)?;
            return Ok(Some((val, should_return)));
        }

        Ok(None)
    }

    fn try_compound(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        let Some(caps) = COMPOUND.captures(expr) else {
            return Ok(None);
        };

        let end = caps.get(0).map(|m| m.end()).unwrap_or(0);
        let rest = &expr[end - 1..];

        if caps.name("if").is_some() {
            let result = self.eval_if(rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        if caps.name("try").is_some() {
            let result = self.eval_try(rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        let is_for = caps.name("for").is_some();
        let is_while = caps.name("while").is_some();

        if is_for || is_while {
            let result = self.eval_loop(is_for, rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        if caps.name("switch").is_some() {
            let result = self.eval_switch(rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        Ok(None)
    }

    fn eval_if(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<(JsValue, bool), JsError> {
        let (cndn, rest) = separate_at_paren(expr, Some(')'))?;

        let (if_expr, mut rest) = split_block_or_line(&rest)?;
        let mut else_expr = None;
        let rest_trim = rest.trim_start().to_owned();

        if let Some(after) = rest_trim.strip_prefix("else") {
            let after = after.trim_start();
            let (body, outer) = split_else_body(after)?;

            else_expr = Some(body);
            rest = outer;
        }

        let cond = self.interpret_expression(&cndn, local, rec)?;
        let mut branch = else_expr;

        if cond.as_bool_js() {
            branch = Some(if_expr);
        }

        if let Some(body) = branch {
            let (ret, abort) = self.interpret_statement(&body, local, rec)?;

            if abort {
                return Ok((ret, true));
            }
        }

        if rest.trim().is_empty() {
            return Ok((JsValue::Null, should_return));
        }

        let (ret, abort) = self.interpret_statement(&rest, local, rec)?;

        Ok((ret, abort || should_return))
    }

    fn eval_try(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<(JsValue, bool), JsError> {
        let (try_expr, mut rest) = separate_at_paren(expr, Some('}'))?;
        let mut err: Option<JsError> = None;
        let mut pending = (JsValue::Null, false);

        match self.interpret_statement(&try_expr, local, rec) {
            Ok((ret, abort)) => {
                if abort {
                    return Ok((ret, true));
                }
            }
            Err(e) => err = Some(e),
        }

        rest = rest.trim_start().to_owned();

        if let Some(caps) = CATCH_RE.captures(&rest) {
            let end = caps.get(0).map(|m| m.end()).unwrap_or(0);
            let err_name = caps.name("err").map(|m| m.as_str().trim().to_owned());
            let catch_src = rest[end - 1..].to_owned();
            let (sub, outer) = separate_at_paren(&catch_src, Some('}'))?;

            rest = outer;

            if let Some(caught) = err.take() {
                let catch_map = Rc::new(RefCell::new(HashMap::new()));

                if let Some(key) = err_name {
                    let thrown = thrown_js(&caught);
                    let slot = EnvSlot::from_value(thrown);
                    catch_map.borrow_mut().insert(key, slot);
                }

                let mut maps = Vec::with_capacity(local.maps.len() + 1);
                maps.push(catch_map);
                maps.extend(local.maps.iter().cloned());
                let catch_local = LocalNs::new(maps);
                pending = self.interpret_statement(&sub, &catch_local, rec)?;
            }
        }

        rest = rest.trim_start().to_owned();

        if let Some(caps) = FINALLY_RE.captures(&rest) {
            let end = caps.get(0).map(|m| m.end()).unwrap_or(0);
            let finally_src = &rest[end - 1..];
            let (sub, _outer) = separate_at_paren(finally_src, Some('}'))?;
            let (ret, abort) = self.interpret_statement(&sub, local, rec)?;

            if abort {
                return Ok((ret, true));
            }
        }

        if pending.1 {
            return Ok(pending);
        }

        if let Some(e) = err {
            return Err(e);
        }

        Ok((pending.0, should_return))
    }

    fn eval_loop(
        &mut self,
        is_for: bool,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<(JsValue, bool), JsError> {
        let (init_or_cond, remaining) = separate_at_paren(expr, Some(')'))?;

        let (body, rest) = split_loop_body(remaining)?;
        let mut cndn = init_or_cond.clone();
        let mut increment = None;

        if is_for {
            let parts = separate(&init_or_cond, ";", None, &[]);
            let start = parts.first().cloned().unwrap_or_default();
            cndn = parts.get(1).cloned().unwrap_or_default();
            increment = parts.get(2).cloned();
            let _ = self.interpret_expression(&start, local, rec)?;
        }

        loop {
            let cond = self.interpret_expression(&cndn, local, rec)?;

            if !cond.as_bool_js() {
                break;
            }

            match self.interpret_statement(&body, local, rec) {
                Ok((ret, abort)) => {
                    if abort {
                        return Ok((ret, true));
                    }
                }
                Err(JsError::Break) => break,
                Err(JsError::Continue) => {}
                Err(e) => return Err(e),
            }

            if let Some(inc) = &increment {
                let _ = self.interpret_expression(inc, local, rec)?;
            }
        }

        if rest.trim().is_empty() {
            return Ok((JsValue::Null, should_return));
        }

        let (ret, abort) = self.interpret_statement(&rest, local, rec)?;

        Ok((ret, abort || should_return))
    }

    fn eval_switch(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<(JsValue, bool), JsError> {
        let (switch_val_expr, remaining) = separate_at_paren(expr, Some(')'))?;

        let switch_val = self.interpret_expression(&switch_val_expr, local, rec)?;
        let (body, rest) = separate_at_paren(&remaining, Some('}'))?;
        let body = body.replace("default:", "case default:");
        let items: Vec<&str> = body.split("case ").skip(1).collect();

        let mut ret = JsValue::Null;
        for default in [false, true] {
            let mut matched = false;
            for item in &items {
                let parts = separate(item.trim(), ":", Some(1), &[]);
                let case = parts.first().map(|s| s.trim()).unwrap_or("");
                let stmt = parts.get(1).map(|s| s.trim()).unwrap_or("");

                if default {
                    matched = matched || case == "default";
                }

                if !default && !matched {
                    let case_val = self.interpret_expression(case, local, rec)?;
                    let is_default = case == "default";
                    matched = !is_default && js_eq(&switch_val, &case_val);
                }

                if !matched {
                    continue;
                }
                match self.interpret_statement(stmt, local, rec) {
                    Ok((v, abort)) => {
                        ret = v;
                        if abort {
                            return Ok((ret, true));
                        }
                    }
                    Err(JsError::Break) => {
                        matched = true;
                        break;
                    }
                    Err(e) => return Err(e),
                }
            }
            if matched {
                break;
            }
        }

        if rest.trim().is_empty() {
            return Ok((ret, should_return));
        }

        let (v, abort) = self.interpret_statement(&rest, local, rec)?;

        Ok((v, abort || should_return))
    }

    fn apply_increments(&mut self, expr: &str, local: &LocalNs) -> String {
        let mut out = expr.to_owned();
        let snapshot = out.clone();

        for caps in INC_RE.captures_iter(&snapshot) {
            let var1 = caps.name("var1");
            let var2 = caps.name("var2");
            let var = var1.or(var2).map(|m| m.as_str()).unwrap_or("");
            let pre = caps.name("pre_sign");
            let post = caps.name("post_sign");
            let sign = pre.or(post).map(|m| m.as_str()).unwrap_or("++");
            let mut ret = local.get(var);
            let one = JsValue::Number(1.0);
            let mut next = js_sub(&ret, &one);

            if sign.starts_with('+') {
                next = js_add(&ret, &one);
            }

            local.set(var, next.clone());

            if pre.is_some() {
                ret = next;
            }

            let dump = self.dump(&ret);

            if let Some(m) = caps.get(0) {
                out = concat3(&snapshot[..m.start()], &dump, &snapshot[m.end()..]);
            }
        }

        out
    }

    fn try_assign(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        let Some(eq_at) = find_assign(expr) else {
            return Ok(None);
        };

        let left = expr[..eq_at].trim();
        let mut op_end = 0;

        for (i, c) in left.char_indices().rev() {
            if OP_CHARS.contains(&c) || c == '=' {
                continue;
            }

            op_end = i + c.len_utf8();
            break;
        }

        let out_part = left[..op_end].trim();
        let op = left[op_end..].trim();
        let right = &expr[eq_at + 1..];

        if let Some((name, idx_src)) = split_index_assign(out_part) {
            let mut left_val = local.get(&name);

            if matches!(left_val, JsValue::Undefined | JsValue::Null) {
                return Err(JsError::msg(format!("cannot index undefined variable {name}")));
            }

            let idx = self.interpret_expression(&idx_src, local, rec)?;
            let idx_key = index_key(&idx);

            if op.is_empty() {
                let new_val = self.interpret_expression(right, local, rec)?;
                index_set(&mut left_val, &idx_key, new_val.clone())?;
                local.set(&name, left_val);

                return Ok(Some((new_val, should_return)));
            }

            let cur = index_get(&left_val, &idx_key)?;
            let dumped = self.dump(&cur);
            let new_val = self.eval_operator(op, &dumped, right, local, rec)?;
            index_set(&mut left_val, &idx_key, new_val.clone())?;
            local.set(&name, left_val);

            return Ok(Some((new_val, should_return)));
        }

        if NAME_RE.is_match(out_part) {
            if op.is_empty() {
                let val = self.interpret_expression(right, local, rec)?;
                local.set(out_part, val.clone());

                return Ok(Some((val, should_return)));
            }

            let left_val = local.get(out_part);
            let dumped = self.dump(&left_val);
            let val = self.eval_operator(op, &dumped, right, local, rec)?;
            local.set(out_part, val.clone());

            return Ok(Some((val, should_return)));
        }

        Ok(None)
    }

    fn try_indexing(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        let Some((name, rest)) = split_name_prefix(expr) else {
            return Ok(None);
        };

        let mut rest = rest.to_owned();

        if !rest.starts_with('[') {
            return Ok(None);
        }

        let mut val = self.lookup(local, name);
        while rest.starts_with('[') {
            let (idx_expr, remaining) = separate_at_paren(&rest, Some(']'))?;
            let idx = self.interpret_expression(&idx_expr, local, rec)?;
            val = index_get(&val, &index_key(&idx))?;
            rest = remaining;
        }

        if !rest.trim().is_empty() {
            return Ok(None);
        }

        Ok(Some((val, should_return)))
    }

    fn try_member(
        &mut self,
        expr: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<Option<(JsValue, bool)>, JsError> {
        let Some((var, rest)) = split_name_prefix(expr) else {
            return Ok(None);
        };

        let rest = rest.trim_start();

        if let Some(after) = rest.strip_prefix("?.") {
            let (member, rest) = split_member_name(after);

            if member.is_empty() {
                return Ok(None);
            }

            let result = self.finish_member(var, &member, true, rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        if let Some(after) = rest.strip_prefix('.') {
            let (member, rest) = split_member_name(after);

            if member.is_empty() {
                return Ok(None);
            }

            let result = self.finish_member(var, &member, false, rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        if rest.starts_with('[') {
            let (idx_expr, after) = separate_at_paren(rest, Some(']'))?;
            let idx = self.interpret_expression(&idx_expr, local, rec)?;
            let member = idx.to_js_string();
            let rest = after.trim_start();
            let result = self.finish_member(var, &member, false, rest, local, rec, should_return)?;

            return Ok(Some(result));
        }

        Ok(None)
    }

    fn finish_member(
        &mut self,
        var: &str,
        member: &str,
        nullish: bool,
        rest: &str,
        local: &LocalNs,
        rec: i32,
        should_return: bool,
    ) -> Result<(JsValue, bool), JsError> {
        let mut arg_str = None;
        let mut remaining = rest.to_owned();

        if rest.starts_with('(') {
            let (inner, rem) = separate_at_paren(rest, Some(')'))?;
            arg_str = Some(inner);
            remaining = rem;
        }

        let val = self.eval_method(var, member, nullish, arg_str.as_deref(), local, rec)?;

        self.continue_with(val, &remaining, local, rec, should_return)
    }

    fn eval_method(
        &mut self,
        variable: &str,
        member: &str,
        nullish: bool,
        arg_str: Option<&str>,
        local: &LocalNs,
        rec: i32,
    ) -> Result<JsValue, JsError> {
        let mut obj = self.lookup(local, variable);
        let undefined = matches!(obj, JsValue::Undefined);
        let builtin = is_js_builtin(variable);

        if undefined && !builtin && nullish {
            return Ok(JsValue::Undefined);
        }

        if undefined && !builtin {
            obj = self.require_object(variable, member, local)?;
        }

        if nullish && matches!(obj, JsValue::Undefined) {
            return Ok(JsValue::Undefined);
        }

        let Some(arg_str) = arg_str else {
            return index_get(&obj, member);
        };

        let mut argvals = Vec::new();

        if !arg_str.is_empty() {
            argvals = self.interpret_iter(arg_str, local, rec)?;
        }

        let date_static = variable == "Date" && matches!(obj, JsValue::Undefined);

        if date_static {
            return date_static_method(member, &argvals);
        }

        if variable == "Math" {
            return math_method(member, &argvals);
        }

        let mut method = member.to_owned();

        if let Some((proto, rest)) = member.split_once('.') {
            if proto == "prototype" {
                apply_prototype_call(&mut method, rest, &mut obj, &mut argvals)?;
            }
        }

        let is_string_ctor = variable == "String" && matches!(obj, JsValue::Undefined);
        let from_char_code = is_string_ctor && method == "fromCharCode";

        if from_char_code {
            return string_from_char_code(&argvals);
        }

        apply_method(self, &mut obj, &method, &argvals, rec)
    }

    fn require_object(
        &mut self,
        variable: &str,
        member: &str,
        local: &LocalNs,
    ) -> Result<JsValue, JsError> {
        if !self.objects.contains_key(variable) {
            if let Ok(extracted) = self.extract_object(variable, local) {
                self.objects.insert(variable.to_owned(), extracted);
            }
        }

        if let Some(o) = self.objects.get(variable) {
            return Ok(o.clone());
        }

        Err(JsError::msg(format!("cannot get index {member}")))
    }

    fn extract_object(&mut self, objname: &str, local: &LocalNs) -> Result<JsValue, JsError> {
        let _ = local;
        let name = regex::escape(objname);
        let pat = format!(
            r"(?s){NAME}\s*\.\s*{name}|{name}\s*=\s*\{{(?P<fields>[^}}]*)\}}\s*;"
        );
        let re = Regex::new(&pat).map_err(|e| JsError::msg(e.to_string()))?;
        let Some(caps) = re.captures(&self.code) else {
            return Err(JsError::msg(format!("could not find object {objname}")));
        };

        let fields = caps.name("fields").map(|m| m.as_str()).unwrap_or("");
        let mut map = HashMap::new();

        for caps in OBJECT_FN.captures_iter(fields) {
            let key = caps.name("key").map(|m| m.as_str()).unwrap_or("");
            let args_txt = caps.name("args").map(|m| m.as_str()).unwrap_or("");
            let args = build_arglist(args_txt);
            let code = caps.name("code").map(|m| m.as_str()).unwrap_or("");
            let source = Rc::clone(&self.code);
            let empty_env = vec![Rc::new(RefCell::new(HashMap::new()))];
            let func = JsFunction::new(key, args, code.to_owned(), source, empty_env, HashMap::new());
            map.insert(key.to_owned(), JsValue::Function(func));
        }

        Ok(JsValue::object(map))
    }

    fn eval_operator(
        &mut self,
        op: &str,
        left_expr: &str,
        right_expr: &str,
        local: &LocalNs,
        rec: i32,
    ) -> Result<JsValue, JsError> {
        let is_unary = matches!(op, "typeof" | "void" | "!");

        if is_unary {
            let left = self.interpret_expression(left_expr.trim(), local, rec)?;

            if op == "void" {
                return Ok(JsValue::Undefined);
            }

            if op == "typeof" {
                return Ok(JsValue::from_str(left.typeof_js()));
            }

            return Ok(JsValue::Bool(!left.as_bool_js()));
        }

        let mut left = JsValue::Null;

        if !left_expr.is_empty() {
            left = self.interpret_expression(left_expr, local, rec)?;
        }

        match op {
            "||" => {
                if left.as_bool_js() {
                    return Ok(left);
                }
            }
            "&&" => {
                if !left.as_bool_js() {
                    return Ok(left);
                }
            }
            "??" => {
                if !left.is_nullish() {
                    return Ok(left);
                }
            }
            "?" => {
                let parts = separate(right_expr, ":", Some(1), &[]);
                let mut chosen = parts.get(1).map(String::as_str).unwrap_or("");

                if left.as_bool_js() {
                    chosen = parts.first().map(String::as_str).unwrap_or("");
                }

                return self.interpret_expression(chosen, local, rec);
            }
            _ => {}
        }

        let mut right = left.clone();

        if !right_expr.is_empty() {
            right = self.interpret_expression(right_expr, local, rec)?;
        }

        Ok(apply_binop(op, &left, &right))
    }

    fn separate_at_op(&self, expr: &str) -> Option<(String, String, String)> {
        let mut expr = expr.to_owned();

        for op in ALL_OPS {
            if *op == "void" || *op == "typeof" || *op == "!" {
                continue;
            }

            let skip: Vec<&str> = match *op {
                "<" | ">" => vec!["<<", ">>"],
                "*" => vec!["**"],
                "?" => vec!["??", "?."],
                _ => Vec::new(),
            };

            let mut separated = separate(&expr, op, None, &skip);
            if separated.len() < 2 {
                continue;
            }

            let mut right = separated.pop().unwrap_or_default();
            let plus_or_minus = *op == "+" || *op == "-";

            if plus_or_minus {
                for s in &mut separated {
                    *s = s.trim().to_owned();
                }

                let mut undone = 0u32;

                while separated.len() > 1 {
                    let last_empty = separated.last().is_some_and(|s| s.is_empty());

                    if !last_empty {
                        break;
                    }

                    undone += 1;
                    separated.pop();
                }

                let odd_minus = *op == "-" && undone % 2 == 1;

                if odd_minus {
                    right = concat2(op, &right);
                }

                if *op == "+" {
                    loop {
                        if separated.len() <= 1 {
                            break;
                        }

                        let Some(last) = separated.last() else {
                            break;
                        };

                        if last.is_empty() {
                            break;
                        }

                        let all_ops = last.chars().all(|c| OP_CHARS.contains(&c));

                        if !all_ops {
                            break;
                        }

                        let Some(prev) = separated.pop() else {
                            break;
                        };

                        right = concat2(&prev, &right);
                    }

                    let ends_with_op = match separated.last() {
                        Some(s) => s.chars().last().is_some_and(|c| OP_CHARS.contains(&c)),
                        None => false,
                    };

                    if ends_with_op {
                        if let Some(prev) = separated.pop() {
                            right = concat2(&prev, &right);
                        }
                    }
                }

                separated.push(right.clone());
                separated = yield_bodmas(separated, op);

                if separated.len() <= 1 {
                    expr = separated.join(op);
                    continue;
                }

                right = separated.pop().unwrap_or_default();
            }

            let left = separated.join(op);

            if left.is_empty() && plus_or_minus {
                return Some(((*op).to_owned(), String::new(), right));
            }

            if left.is_empty() {
                continue;
            }

            return Some(((*op).to_owned(), left, right));
        }

        None
    }
}

fn blocked_return(local: &LocalNs, val: &JsValue) -> bool {
    let blocked = local.get("_ytdl_do_not_return");

    if matches!(blocked, JsValue::Undefined) {
        return false;
    }

    *val == blocked
}

fn object_key(
    interp: &mut JSInterpreter,
    raw: &str,
    local: &LocalNs,
    rec: i32,
) -> Result<String, JsError> {
    if NAME_RE.is_match(raw) {
        return Ok(raw.to_owned());
    }

    let val = interp.interpret_expression(raw, local, rec)?;

    Ok(val.to_js_string())
}

fn split_block_or_line(rest: &str) -> Result<(String, String), JsError> {
    if rest.starts_with('{') {
        return separate_at_paren(rest, Some('}'));
    }

    let mut padded = String::with_capacity(rest.len() + 2);
    padded.push(' ');
    padded.push_str(rest);
    padded.push(';');

    separate_at_paren(&padded, Some(';'))
}

fn split_else_body(after: &str) -> Result<(String, String), JsError> {
    if after.starts_with('{') {
        return separate_at_paren(after, Some('}'));
    }

    Ok((after.trim().to_owned(), String::new()))
}

fn split_loop_body(remaining: String) -> Result<(String, String), JsError> {
    if remaining.starts_with('{') {
        return separate_at_paren(&remaining, Some('}'));
    }

    Ok((remaining, String::new()))
}

fn split_member_name(rest: &str) -> (String, &str) {
    let end = rest.find('(').unwrap_or(rest.len());
    let member = rest[..end].trim().to_owned();

    (member, &rest[end..])
}

fn thrown_js(err: &JsError) -> JsValue {
    match err {
        JsError::Throw(s) => JsValue::from_str(s.clone()),
        other => JsValue::from_str(other.to_string()),
    }
}

fn is_js_builtin(variable: &str) -> bool {
    matches!(variable, "String" | "Math" | "Array" | "Date" | "RegExp")
}

fn date_static_method(member: &str, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    match member {
        "now" => Ok(JsValue::Number(date::now_ms())),
        "parse" => {
            let s = argvals.first().map(JsValue::to_js_string).unwrap_or_default();

            Ok(JsValue::Number(date::parse(&s).unwrap_or(f64::NAN)))
        }
        "UTC" => Ok(JsValue::Number(date::utc_ms(argvals))),
        _ => Err(JsError::msg("unsupported Date method")),
    }
}

fn math_method(member: &str, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    if member != "pow" {
        return Err(JsError::msg("unsupported Math method"));
    }

    if argvals.len() != 2 {
        return Err(JsError::msg("takes two arguments"));
    }

    Ok(js_exp(&argvals[0], &argvals[1]))
}

fn apply_prototype_call(
    method: &mut String,
    rest: &str,
    obj: &mut JsValue,
    argvals: &mut Vec<JsValue>,
) -> Result<(), JsError> {
    let (meth, call_kind) = rest.split_once('.').unwrap_or((rest, ""));
    *method = meth.to_owned();

    if call_kind == "call" {
        *obj = argvals.remove(0);
        return Ok(());
    }

    if call_kind != "apply" {
        return Ok(());
    }

    if argvals.len() != 2 {
        return Err(JsError::msg("takes two arguments"));
    }

    *obj = argvals.remove(0);
    let applied = match argvals.first() {
        Some(JsValue::Array(a)) => a.borrow().clone(),
        _ => return Err(JsError::msg("second argument must be a list")),
    };

    *argvals = applied;

    Ok(())
}

fn string_from_char_code(argvals: &[JsValue]) -> Result<JsValue, JsError> {
    let mut s = String::new();

    for n in argvals {
        let code = n.to_number() as u32;
        let Some(c) = char::from_u32(code) else {
            continue;
        };

        s.push(c);
    }

    Ok(JsValue::from_str(s))
}

fn yield_bodmas(terms: Vec<String>, op: &str) -> Vec<String> {
    let dm = ["*", "%", "/", "**"];
    let mut out = Vec::new();
    let mut skip = false;
    let last = terms.len().saturating_sub(1);

    for (i, term) in terms.iter().enumerate() {
        if skip {
            skip = false;
            continue;
        }
        if i == last {
            if !term.is_empty() {
                out.push(term.clone());
            }
            break;
        }

        let has_dm = dm.iter().any(|d| term.contains(d));
        if !has_dm {
            out.push(term.clone());
            continue;
        }

        let mut folded = false;
        for dm_op in dm {
            let bodmas = separate(term, dm_op, None, &[]);
            if bodmas.len() > 1 && bodmas.last().is_some_and(|s| s.trim().is_empty()) {
                let mut bodmas = bodmas;
                let mut suffix = terms.get(i + 1).cloned().unwrap_or_default();

                if op == "-" {
                    suffix = concat2(op, &suffix);
                }

                if let Some(last) = bodmas.last_mut() {
                    *last = suffix;
                }

                out.push(bodmas.join(dm_op));
                skip = true;
                folded = true;
                break;
            }
        }
        if !folded && !term.is_empty() {
            out.push(term.clone());
        }
    }

    out
}

fn apply_binop(op: &str, left: &JsValue, right: &JsValue) -> JsValue {
    match op {
        "+" => js_add(left, right),
        "-" => js_sub(left, right),
        "*" => js_mul(left, right),
        "/" => js_div(left, right),
        "%" => js_mod(left, right),
        "**" => js_exp(left, right),
        "<<" => {
            let v = (js_to_int32(left) as u32).wrapping_shl(js_shift_count(right)) as i32;
            JsValue::Number(f64::from(v))
        }
        ">>" => {
            let v = js_to_int32(left).wrapping_shr(js_shift_count(right));
            JsValue::Number(f64::from(v))
        }
        "&" => JsValue::Number(f64::from(js_to_int32(left) & js_to_int32(right))),
        "|" => JsValue::Number(f64::from(js_to_int32(left) | js_to_int32(right))),
        "^" => JsValue::Number(f64::from(js_to_int32(left) ^ js_to_int32(right))),
        "==" => JsValue::Bool(js_eq(left, right)),
        "!=" => JsValue::Bool(!js_eq(left, right)),
        "===" => JsValue::Bool(js_strict_eq(left, right)),
        "!==" => JsValue::Bool(!js_strict_eq(left, right)),
        "<" => JsValue::Bool(js_cmp(left, right, CmpOp::Lt)),
        "<=" => JsValue::Bool(js_cmp(left, right, CmpOp::Le)),
        ">" => JsValue::Bool(js_cmp(left, right, CmpOp::Gt)),
        ">=" => JsValue::Bool(js_cmp(left, right, CmpOp::Ge)),
        "||" | "&&" | "??" => right.clone(),
        _ => right.clone(),
    }
}

fn apply_method(
    interp: &mut JSInterpreter,
    obj: &mut JsValue,
    member: &str,
    argvals: &[JsValue],
    rec: i32,
) -> Result<JsValue, JsError> {
    match member {
        "split" => method_split(obj, argvals),
        "join" => method_join(obj, argvals),
        "reverse" => {
            let JsValue::Array(a) = obj else {
                return Err(JsError::msg("must be applied on a list"));
            };

            a.borrow_mut().reverse();
            Ok(obj.clone())
        }
        "slice" => method_slice(obj, argvals),
        "splice" => method_splice(obj, argvals),
        "pop" => method_pop_shift(obj, false),
        "shift" => method_pop_shift(obj, true),
        "push" => {
            let JsValue::Array(a) = obj else {
                return Err(JsError::msg("must be applied on a list"));
            };

            a.borrow_mut().extend_from_slice(argvals);
            let len = a.borrow().len();
            Ok(JsValue::Number(len as f64))
        }
        "unshift" => {
            let JsValue::Array(a) = obj else {
                return Err(JsError::msg("must be applied on a list"));
            };

            let mut arr = a.borrow_mut();

            for (i, v) in argvals.iter().enumerate() {
                arr.insert(i, v.clone());
            }

            let len = arr.len();
            Ok(JsValue::Number(len as f64))
        }
        "forEach" => method_for_each(interp, obj, argvals, rec),
        "charCodeAt" => method_char_code_at(obj, argvals),
        "replace" | "replaceAll" => method_replace(obj, member, argvals),
        "source" | "pattern" => {
            if let JsValue::Regex(re) = obj {
                return Ok(JsValue::from_str(re.source()));
            }

            index_get(obj, member)
        }
        "length" => match obj {
            JsValue::Array(a) => Ok(JsValue::Number(a.borrow().len() as f64)),
            JsValue::String(s) => Ok(JsValue::Number(s.chars().count() as f64)),
            _ => index_get(obj, member),
        },
        _ => {
            if let JsValue::Function(func) = index_get(obj, member)? {
                return interp.run_function(&func, argvals, None, rec);
            }

            index_get(obj, member)
        }
    }
}

fn method_split(obj: &JsValue, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    let JsValue::String(s) = obj else {
        return Err(JsError::msg("must be applied on a string"));
    };

    let limit = match argvals.get(1) {
        Some(JsValue::Number(n)) if *n == 0.0 => return Ok(JsValue::array(Vec::new())),
        Some(JsValue::Number(n)) => Some(*n as usize),
        _ => None,
    };

    if argvals.is_empty() || matches!(argvals.first(), Some(JsValue::Undefined)) {
        let mut chars: Vec<JsValue> = s.chars().map(|c| JsValue::from_str(c.to_string())).collect();
        if let Some(lim) = limit {
            chars.truncate(lim);
        }
        return Ok(JsValue::array(chars));
    }

    match argvals.first() {
        Some(JsValue::Regex(re)) => {
            let mut parts = Vec::new();
            let mut last = 0usize;
            for m in re.compiled().find_iter(s) {
                if m.start() == m.end() && m.start() == 0 {
                    continue;
                }
                parts.push(JsValue::from_str(&s[last..m.start()]));
                last = m.end();
                if let Some(lim) = limit {
                    if parts.len() + 1 >= lim {
                        break;
                    }
                }
            }
            if last < s.len() {
                parts.push(JsValue::from_str(&s[last..]));
            }
            if s.is_empty() {
                return Ok(JsValue::array(Vec::new()));
            }
            Ok(JsValue::array(parts))
        }
        Some(JsValue::String(sep)) if sep.is_empty() => {
            if s.is_empty() {
                return Ok(JsValue::array(Vec::new()));
            }
            let chars: Vec<JsValue> = s.chars().map(|c| JsValue::from_str(c.to_string())).collect();
            Ok(JsValue::array(chars))
        }
        Some(JsValue::String(sep)) => {
            if s.is_empty() {
                return Ok(JsValue::array(vec![JsValue::from_str("")]));
            }

            let pieces = s.split(sep.as_ref());
            let mut parts = Vec::new();

            for piece in pieces {
                parts.push(JsValue::from_str(piece));
            }

            Ok(JsValue::array(parts))
        }
        _ => {
            let chars: Vec<JsValue> = s.chars().map(|c| JsValue::from_str(c.to_string())).collect();
            Ok(JsValue::array(chars))
        }
    }
}

fn method_join(obj: &JsValue, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    let JsValue::Array(a) = obj else {
        return Err(JsError::msg("must be applied on a list"));
    };

    let sep = match argvals.first() {
        None | Some(JsValue::Null | JsValue::Undefined) => ",",
        Some(v) => {
            return Ok(JsValue::from_str(join_with(&a.borrow(), &v.to_js_string())));
        }
    };

    Ok(JsValue::from_str(join_with(&a.borrow(), sep)))
}

fn join_with(items: &[JsValue], sep: &str) -> String {
    let mut out = String::with_capacity(items.len().saturating_mul(sep.len() + 1));

    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(sep);
        }

        if matches!(item, JsValue::Null | JsValue::Undefined) {
            continue;
        }

        out.push_str(&item.to_js_string());
    }

    out
}

fn method_slice(obj: &JsValue, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    let start = argvals.first().map(|v| v.to_number() as i64);
    let end = argvals.get(1).map(|v| v.to_number() as i64);

    match obj {
        JsValue::Array(a) => {
            let arr = a.borrow();
            let (s, e) = slice_bounds(arr.len(), start, end);
            Ok(JsValue::array(arr[s..e].to_vec()))
        }
        JsValue::String(s) => {
            let chars: Vec<char> = s.chars().collect();
            let (a, b) = slice_bounds(chars.len(), start, end);
            Ok(JsValue::from_str(chars[a..b].iter().collect::<String>()))
        }
        _ => Err(JsError::msg("must be applied on a list or string")),
    }
}

fn slice_bounds(len: usize, start: Option<i64>, end: Option<i64>) -> (usize, usize) {
    let to_idx = |n: i64| -> usize {
        if n < 0 {
            return (len as i64 + n).max(0) as usize;
        }
        (n as usize).min(len)
    };

    let s = start.map(to_idx).unwrap_or(0);
    let e = end.map(to_idx).unwrap_or(len);
    if s > e {
        return (s, s);
    }
    (s, e)
}

fn method_splice(obj: &mut JsValue, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    let JsValue::Array(a) = obj else {
        return Err(JsError::msg("must be applied on a list"));
    };

    let mut arr = a.borrow_mut();
    let mut index = argvals.first().map(|v| v.to_number() as i64).unwrap_or(0);
    let mut how_many = arr.len() as i64;

    if let Some(v) = argvals.get(1) {
        how_many = v.to_number() as i64;
    }

    if index < 0 {
        index += arr.len() as i64;
    }

    let index = index.max(0) as usize;
    let n = how_many.max(0) as usize;
    let mut removed = Vec::new();

    for _ in 0..n {
        if index < arr.len() {
            removed.push(arr.remove(index));
        }
    }

    let mut insert: &[JsValue] = &[];

    if argvals.len() > 2 {
        insert = &argvals[2..];
    }

    for (i, v) in insert.iter().enumerate() {
        arr.insert(index + i, v.clone());
    }

    Ok(JsValue::array(removed))
}

fn method_pop_shift(obj: &mut JsValue, shift: bool) -> Result<JsValue, JsError> {
    let JsValue::Array(a) = obj else {
        return Err(JsError::msg("must be applied on a list"));
    };

    let mut arr = a.borrow_mut();
    if arr.is_empty() {
        return Ok(JsValue::Undefined);
    }

    if shift {
        return Ok(arr.remove(0));
    }

    Ok(arr.pop().unwrap_or(JsValue::Undefined))
}

fn method_for_each(
    interp: &mut JSInterpreter,
    obj: &JsValue,
    argvals: &[JsValue],
    rec: i32,
) -> Result<JsValue, JsError> {
    let JsValue::Array(a) = obj else {
        return Err(JsError::msg("must be applied on a list"));
    };

    let Some(JsValue::Function(func)) = argvals.first() else {
        return Err(JsError::msg("takes one or more arguments"));
    };

    let this = argvals.get(1).cloned().unwrap_or(JsValue::from_str(""));
    let mut kwargs = HashMap::with_capacity(1);
    kwargs.insert(String::from("this"), this);
    let len = a.borrow().len();

    for idx in 0..len {
        let item = a.borrow().get(idx).cloned().unwrap_or(JsValue::Undefined);
        let args = [item, JsValue::Number(idx as f64), obj.clone()];
        let _ = interp.run_function(func, &args, Some(&kwargs), rec)?;
    }

    Ok(obj.clone())
}

fn method_char_code_at(obj: &JsValue, argvals: &[JsValue]) -> Result<JsValue, JsError> {
    let JsValue::String(s) = obj else {
        return Err(JsError::msg("must be applied on a string"));
    };

    let idx = match argvals.first() {
        Some(JsValue::Number(n)) if n.fract() == 0.0 => *n as i64,
        _ => 0,
    };

    if idx < 0 {
        return Ok(JsValue::Null);
    }

    let Some(c) = s.chars().nth(idx as usize) else {
        return Ok(JsValue::Null);
    };

    Ok(JsValue::Number(f64::from(c as u32)))
}

fn method_replace(
    obj: &JsValue,
    member: &str,
    argvals: &[JsValue],
) -> Result<JsValue, JsError> {
    let JsValue::String(s) = obj else {
        return Err(JsError::msg("must be applied on a string"));
    };

    if argvals.len() != 2 {
        return Err(JsError::msg("takes exactly two arguments"));
    }

    let repl = argvals[1].to_js_string();

    match &argvals[0] {
        JsValue::Regex(re) => {
            let global = re.global();

            if member == "replaceAll" && !global {
                return Err(JsError::msg(
                    "replaceAll must be called with a global RegExp",
                ));
            }

            if global {
                let out = re.compiled().replace_all(s, repl.as_str()).into_owned();

                return Ok(JsValue::from_str(out));
            }

            let out = re.compiled().replace(s, repl.as_str()).into_owned();

            Ok(JsValue::from_str(out))
        }
        other => {
            let pat = other.to_js_string();

            if member == "replaceAll" {
                return Ok(JsValue::from_str(s.replace(&pat, &repl)));
            }

            Ok(JsValue::from_str(s.replacen(&pat, &repl, 1)))
        }
    }
}

fn index_get(obj: &JsValue, idx: &str) -> Result<JsValue, JsError> {
    match obj {
        JsValue::Array(a) => {
            if idx == "length" {
                return Ok(JsValue::Number(a.borrow().len() as f64));
            }
            let n: f64 = idx.parse().unwrap_or(f64::NAN);
            if n.is_nan() {
                return Ok(JsValue::Undefined);
            }
            let i = n as i64;
            if i < 0 {
                return Ok(JsValue::Undefined);
            }
            Ok(a.borrow().get(i as usize).cloned().unwrap_or(JsValue::Undefined))
        }
        JsValue::String(s) => {
            if idx == "length" {
                return Ok(JsValue::Number(s.chars().count() as f64));
            }
            Ok(JsValue::Undefined)
        }
        JsValue::Object(o) => Ok(o.borrow().get(idx).cloned().unwrap_or(JsValue::Undefined)),
        JsValue::Regex(re) if idx == "source" || idx == "pattern" => {
            Ok(JsValue::from_str(re.source()))
        }
        JsValue::Undefined | JsValue::Null => {
            Err(JsError::msg(format!("cannot get index {idx}")))
        }
        _ => Ok(JsValue::Undefined),
    }
}

fn index_set(obj: &mut JsValue, idx: &str, val: JsValue) -> Result<(), JsError> {
    match obj {
        JsValue::Array(a) => {
            let n: f64 = idx.parse().unwrap_or(0.0);
            let i = n as usize;
            let mut arr = a.borrow_mut();
            while arr.len() <= i {
                arr.push(JsValue::Undefined);
            }
            arr[i] = val;
            Ok(())
        }
        JsValue::Object(o) => {
            o.borrow_mut().insert(idx.to_owned(), val);
            Ok(())
        }
        _ => Err(JsError::msg("cannot index")),
    }
}

fn index_key(idx: &JsValue) -> String {
    match idx {
        JsValue::Number(n) if n.fract() == 0.0 => js_number_string(*n),
        other => other.to_js_string(),
    }
}

fn find_assign(expr: &str) -> Option<usize> {
    let bytes = expr.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let is_eq = bytes[i] == b'=';
        let not_eqeq = bytes.get(i + 1).is_none_or(|&b| b != b'=');
        let is_assign = is_eq && not_eqeq && i > 0;

        if is_assign {
            let prev = bytes[i - 1];
            let is_cmp = prev == b'=' || prev == b'!' || prev == b'<' || prev == b'>';

            if !is_cmp {
                return Some(i);
            }
        }

        i += 1;
    }

    None
}

fn split_index_assign(left: &str) -> Option<(String, String)> {
    let (name, rest) = split_name_prefix(left)?;

    if !rest.starts_with('[') {
        return None;
    }

    let (inner, remaining) = separate_at_paren(rest, Some(']')).ok()?;

    if !remaining.trim().is_empty() {
        return None;
    }

    Some((name.to_owned(), inner))
}

fn split_name_prefix(expr: &str) -> Option<(&str, &str)> {
    let expr = expr.trim();
    let mut end = 0usize;

    for (i, c) in expr.char_indices() {
        let mut ident = c.is_ascii_alphanumeric() || c == '_' || c == '$';

        if i == 0 {
            ident = c.is_ascii_alphabetic() || c == '_' || c == '$';
        }

        if !ident {
            break;
        }

        end = i + c.len_utf8();
    }

    if end == 0 {
        return None;
    }

    Some((&expr[..end], &expr[end..]))
}

fn split_call(expr: &str) -> Option<(&str, String)> {
    let (name, rest) = split_name_prefix(expr)?;

    if !rest.starts_with('(') {
        return None;
    }

    let is_keyword = matches!(name, "if" | "return" | "true" | "false" | "null" | "undefined" | "NaN" | "Infinity" | "void" | "typeof");

    if is_keyword {
        return None;
    }

    let Ok((args, remaining)) = separate_at_paren(rest, Some(')')) else {
        return None;
    };

    if remaining.trim().is_empty() {
        return Some((name, args));
    }

    None
}

fn build_arglist(arg_text: &str) -> Vec<String> {
    if arg_text.trim().is_empty() {
        return Vec::new();
    }

    let parts = comma_split(arg_text);
    let mut args = Vec::with_capacity(parts.len());

    for part in parts {
        let name = part.trim();

        if name.is_empty() {
            continue;
        }

        args.push(name.to_owned());
    }

    args
}

fn stmt_prefix(stmt: &str) -> Option<StmtPrefix> {
    for kw in ["var", "const", "let"] {
        let Some(rest) = stmt.strip_prefix(kw) else {
            continue;
        };
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }

        return Some(StmtPrefix {
            end: kw.len() + leading_ws_len(rest),
            ret: false,
            throw: false,
        });
    }

    if let Some(rest) = stmt.strip_prefix("return") {
        let quoted = rest.starts_with('"') || rest.starts_with('\'');
        let spaced = rest.starts_with(|c: char| c.is_whitespace());
        if rest.is_empty() || quoted || spaced {
            let mut extra = 0;

            if spaced {
                extra = leading_ws_len(rest);
            }

            return Some(StmtPrefix {
                end: 6 + extra,
                ret: true,
                throw: false,
            });
        }
    }

    if let Some(rest) = stmt.strip_prefix("throw") {
        if rest.starts_with(|c: char| c.is_whitespace()) {
            return Some(StmtPrefix {
                end: 5 + leading_ws_len(rest),
                ret: false,
                throw: true,
            });
        }
    }

    None
}

fn leading_ws_len(s: &str) -> usize {
    let ws = s.chars().take_while(|c| c.is_whitespace());

    ws.map(char::len_utf8).sum()
}

struct StmtPrefix {
    end: usize,
    ret: bool,
    throw: bool,
}

fn try_literal(expr: &str) -> Option<JsValue> {
    let expr = expr.trim();
    match expr {
        "true" => return Some(JsValue::Bool(true)),
        "false" => return Some(JsValue::Bool(false)),
        "null" => return Some(JsValue::Null),
        _ => {}
    }

    if let Ok(n) = expr.parse::<f64>() {
        if expr.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-' || c == '+') {
            return Some(JsValue::Number(n));
        }
    }

    parse_js_string(expr)
}

fn parse_js_string(expr: &str) -> Option<JsValue> {
    let mut chars = expr.chars();
    let q = chars.next()?;

    if q != '\'' && q != '"' {
        return None;
    }

    let mut out = String::new();
    let mut escaped = false;
    for c in chars {
        if escaped {
            let ch = match c {
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                '\\' => '\\',
                '\'' => '\'',
                '"' => '"',
                other => other,
            };
            out.push(ch);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == q {
            return Some(JsValue::from_str(out));
        }
        out.push(c);
    }

    None
}

fn regex_flags(expr: &str) -> (u32, String) {
    let mut flags = 0u32;
    let mut end = 0usize;

    for (i, c) in expr.char_indices() {
        match c {
            'g' => flags |= RE_G,
            'i' => flags |= RE_I,
            'm' => flags |= RE_M,
            's' => flags |= RE_S,
            'u' | 'v' => flags |= RE_U,
            _ => return (flags, expr[i..].to_owned()),
        }

        end = i + c.len_utf8();
    }

    (flags, expr[end..].to_owned())
}

fn concat2(a: &str, b: &str) -> String {
    let mut out = String::with_capacity(a.len() + b.len());
    out.push_str(a);
    out.push_str(b);
    out
}

fn concat3(a: &str, b: &str, c: &str) -> String {
    let mut out = String::with_capacity(a.len() + b.len() + c.len());
    out.push_str(a);
    out.push_str(b);
    out.push_str(c);
    out
}

fn named_var(n: u32) -> String {
    let mut name = String::with_capacity(OBJ_PREFIX.len() + 10);
    name.push_str(OBJ_PREFIX);
    push_u32(&mut name, n);
    name
}

fn u32_string(n: u32) -> String {
    let mut out = String::with_capacity(10);
    push_u32(&mut out, n);
    out
}

fn push_u32(buf: &mut String, mut n: u32) {
    if n == 0 {
        buf.push('0');
        return;
    }

    let mut tmp = [0u8; 10];
    let mut i = 10;

    while n > 0 {
        i -= 1;
        tmp[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    match std::str::from_utf8(&tmp[i..]) {
        Ok(digits) => buf.push_str(digits),
        Err(_) => buf.push('0'),
    }
}

fn flags_from_value(v: &JsValue) -> u32 {
    match v {
        JsValue::String(s) => regex_flags(s).0,
        JsValue::Number(n) => *n as u32,
        _ => 0,
    }
}
