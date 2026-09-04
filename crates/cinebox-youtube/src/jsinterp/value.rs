//! JS runtime values. Primitives stay off the heap.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::{Rc, Weak};

use regex::Regex;

use super::JsError;

pub(super) type EnvMap = HashMap<String, EnvSlot>;
pub(super) type EnvRc = Rc<RefCell<EnvMap>>;
pub(super) type ObjMap = HashMap<String, JsValue>;

/// Env-map slot. Functions are weak so a closure does not pin its own map.
pub(super) enum EnvSlot {
    Value(JsValue),
    WeakFn(Weak<JsFunctionInner>),
}

impl EnvSlot {
    pub(super) fn from_value(val: JsValue) -> Self {
        match val {
            JsValue::Function(func) => Self::WeakFn(func.downgrade()),
            other => Self::Value(other),
        }
    }

    pub(super) fn to_value(&self) -> JsValue {
        match self {
            Self::WeakFn(weak) => match weak.upgrade() {
                Some(inner) => JsValue::Function(JsFunction::from_inner(inner)),
                None => JsValue::Undefined,
            },
            Self::Value(val) => val.clone(),
        }
    }
}

/// Compiled `/pattern/flags`.
#[derive(Debug, Clone)]
pub struct JsRegex {
    source: String,
    flags: u32,
    compiled: Regex,
}

pub(super) const RE_G: u32 = 1 << 11;
pub(super) const RE_I: u32 = 2;
pub(super) const RE_M: u32 = 8;
pub(super) const RE_S: u32 = 16;
pub(super) const RE_U: u32 = 32;

impl JsRegex {
    pub fn new(source: &str, flags: u32) -> Result<Self, JsError> {
        let mut source = source.replace("[[", r"\[");

        if source.is_empty() {
            source = String::from("(?:)");
        }

        let mut builder = regex::RegexBuilder::new(&source);

        if flags & RE_I != 0 {
            builder.case_insensitive(true);
        }

        if flags & RE_M != 0 {
            builder.multi_line(true);
        }

        if flags & RE_S != 0 {
            builder.dot_matches_new_line(true);
        }

        builder.unicode(true);

        let compiled = match builder.build() {
            Ok(re) => re,
            Err(err) => return Err(JsError::msg(err.to_string())),
        };

        Ok(Self {
            source,
            flags,
            compiled,
        })
    }

    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub(super) fn source(&self) -> &str {
        &self.source
    }

    #[must_use]
    pub fn ignore_case(&self) -> bool {
        self.flags & RE_I != 0
    }

    #[must_use]
    pub fn global(&self) -> bool {
        self.flags & RE_G != 0
    }

    #[must_use]
    pub fn compiled(&self) -> &Regex {
        &self.compiled
    }

    #[must_use]
    pub(super) fn flags(&self) -> u32 {
        self.flags
    }
}

pub(super) struct JsFunctionInner {
    repr: String,
    argnames: Rc<[String]>,
    body: Rc<str>,
    source: Rc<str>,
    closure: Vec<EnvRc>,
    nested: HashMap<String, JsFunction>,
}

/// Callable extracted from JS source. Cheap to clone (`Rc`).
#[derive(Clone)]
pub struct JsFunction {
    inner: Rc<JsFunctionInner>,
}

impl fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.repr)
    }
}

impl fmt::Display for JsFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.inner.repr)
    }
}

impl JsFunction {
    pub(super) fn new(
        name: &str,
        argnames: Vec<String>,
        body: String,
        source: Rc<str>,
        closure: Vec<EnvRc>,
        nested: HashMap<String, JsFunction>,
    ) -> Self {
        let mut repr = String::with_capacity(3 + name.len());
        repr.push_str("F<");
        repr.push_str(name);
        repr.push('>');

        Self {
            inner: Rc::new(JsFunctionInner {
                repr,
                argnames: argnames.into(),
                body: body.into(),
                source,
                closure,
                nested,
            }),
        }
    }

    #[must_use]
    pub(super) fn argnames(&self) -> &[String] {
        &self.inner.argnames
    }

    #[must_use]
    pub(super) fn body(&self) -> &str {
        &self.inner.body
    }

    #[must_use]
    pub(super) fn source(&self) -> Rc<str> {
        Rc::clone(&self.inner.source)
    }

    #[must_use]
    pub(super) fn closure(&self) -> &[EnvRc] {
        &self.inner.closure
    }

    #[must_use]
    pub(super) fn nested(&self) -> &HashMap<String, JsFunction> {
        &self.inner.nested
    }

    #[must_use]
    pub(super) fn repr(&self) -> &str {
        &self.inner.repr
    }

    fn from_inner(inner: Rc<JsFunctionInner>) -> Self {
        Self { inner }
    }

    fn downgrade(&self) -> Weak<JsFunctionInner> {
        Rc::downgrade(&self.inner)
    }

    /// Invoke with a JS argument list (`extract_function('a')([2])`).
    ///
    /// # Errors
    ///
    /// Interpreter failures.
    pub fn call(&self, args: &[JsValue]) -> Result<JsValue, JsError> {
        crate::jsinterp::JSInterpreter::call_extracted(self, args, None, 100)
    }
}

/// One JS value. Heap only for string/array/object/function/regex.
#[derive(Clone, Debug)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<str>),
    Array(Rc<RefCell<Vec<JsValue>>>),
    Object(Rc<RefCell<ObjMap>>),
    Function(JsFunction),
    Regex(Rc<JsRegex>),
}

const _: () = assert!(std::mem::size_of::<JsValue>() <= 24);

impl JsValue {
    #[must_use]
    pub fn nan() -> Self {
        Self::Number(f64::NAN)
    }

    #[must_use]
    pub fn infinity() -> Self {
        Self::Number(f64::INFINITY)
    }

    #[must_use]
    pub fn from_str(s: impl Into<String>) -> Self {
        Self::String(s.into().into())
    }

    #[must_use]
    pub fn array(items: Vec<JsValue>) -> Self {
        Self::Array(Rc::new(RefCell::new(items)))
    }

    #[must_use]
    pub fn object(map: ObjMap) -> Self {
        Self::Object(Rc::new(RefCell::new(map)))
    }

    #[must_use]
    pub fn is_nan(&self) -> bool {
        matches!(self, Self::Number(n) if n.is_nan())
    }

    #[must_use]
    pub fn is_nullish(&self) -> bool {
        matches!(self, Self::Null | Self::Undefined)
    }

    #[must_use]
    pub fn same_object(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Array(a), Self::Array(b)) => Rc::ptr_eq(a, b),
            (Self::Object(a), Self::Object(b)) => Rc::ptr_eq(a, b),
            (Self::Regex(a), Self::Regex(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }

    #[must_use]
    pub fn as_bool_js(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => !n.is_nan() && *n != 0.0,
            Self::String(s) => !s.is_empty(),
            Self::Array(_)
            | Self::Object(_)
            | Self::Function(_)
            | Self::Regex(_) => true,
        }
    }

    #[must_use]
    pub fn to_js_string(&self) -> String {
        match self {
            Self::Undefined => String::from("undefined"),
            Self::Null => String::from("null"),
            Self::Bool(true) => String::from("true"),
            Self::Bool(false) => String::from("false"),
            Self::Number(n) if n.is_nan() => String::from("NaN"),
            Self::Number(n) if n.is_infinite() && n.is_sign_positive() => {
                String::from("Infinity")
            }
            Self::Number(n) if n.is_infinite() => String::from("-Infinity"),
            Self::Number(n) => js_number_string(*n),
            Self::String(s) => s.to_string(),
            Self::Array(items) => join_js_array(&items.borrow()),
            Self::Object(_) => String::from("[object Object]"),
            Self::Function(func) => func.repr().to_owned(),
            Self::Regex(re) => regex_literal(re.source()),
        }
    }

    #[must_use]
    pub fn to_number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Null | Self::Bool(false) => 0.0,
            Self::Bool(true) => 1.0,
            Self::Number(n) => *n,
            Self::String(s) => parse_js_number(s),
            Self::Array(_)
            | Self::Object(_)
            | Self::Function(_)
            | Self::Regex(_) => f64::NAN,
        }
    }

    #[must_use]
    pub fn typeof_js(&self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "object",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Function(_) => "function",
            Self::Array(_) | Self::Object(_) | Self::Regex(_) => "object",
        }
    }
}

fn join_js_array(items: &[JsValue]) -> String {
    let mut out = String::with_capacity(items.len().saturating_mul(2));

    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }

        out.push_str(&item.to_js_string());
    }

    out
}

fn regex_literal(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + 2);
    out.push('/');
    out.push_str(source);
    out.push('/');
    out
}

fn parse_js_number(s: &str) -> f64 {
    let s = s.trim();

    if s.is_empty() {
        return 0.0;
    }

    s.parse().unwrap_or(f64::NAN)
}

pub(super) fn js_number_string(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        return format!("{n:.0}");
    }

    let raw = format!("{n:.7}");
    let trimmed = raw.trim_end_matches('0');
    let trimmed = trimmed.trim_end_matches('.');
    trimmed.to_owned()
}

/// Structural equality for tests. JS `==` uses [`js_eq`].
impl PartialEq for JsValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a.is_nan() && b.is_nan() || a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Array(a), Self::Array(b)) => a.borrow().as_slice() == b.borrow().as_slice(),
            (Self::Object(a), Self::Object(b)) => *a.borrow() == *b.borrow(),
            (Self::Function(a), Self::Function(b)) => a.repr() == b.repr(),
            (Self::Regex(a), Self::Regex(b)) => {
                a.source() == b.source() && a.flags() == b.flags()
            }
            _ => false,
        }
    }
}

/// youtube-dl `_js_eq`.
#[must_use]
pub fn js_eq(a: &JsValue, b: &JsValue) -> bool {
    if a.is_nan() || b.is_nan() {
        return false;
    }

    if a.same_object(b) {
        return true;
    }

    let a_obj = matches!(a, JsValue::Array(_) | JsValue::Object(_));
    let b_obj = matches!(b, JsValue::Array(_) | JsValue::Object(_));

    if a_obj && b_obj {
        return false;
    }

    if js_same_primitive(a, b) {
        return true;
    }

    if a.is_nullish() && b.is_nullish() {
        return true;
    }

    let pa = js_primitive(a);
    let pb = js_primitive(b);

    match (&pa, &pb) {
        (JsValue::String(s), other) | (other, JsValue::String(s)) => {
            let n: f64 = s.trim().parse().unwrap_or(f64::NAN);

            if n.is_nan() {
                return false;
            }

            let other_n = other.to_number();
            (other_n - n).abs() < f64::EPSILON || other_n == n
        }
        _ => js_same_primitive(&pa, &pb),
    }
}

fn js_same_primitive(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Undefined, JsValue::Undefined) | (JsValue::Null, JsValue::Null) => true,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::String(x), JsValue::String(y)) => x == y,
        _ => false,
    }
}

fn js_primitive(v: &JsValue) -> JsValue {
    match v {
        JsValue::Array(_) => JsValue::from_str(v.to_js_string()),
        JsValue::Object(_) => JsValue::from_str("[object Object]"),
        other => other.clone(),
    }
}

/// youtube-dl `_js_id_op` for `===`.
#[must_use]
pub fn js_strict_eq(a: &JsValue, b: &JsValue) -> bool {
    if a.is_nan() || b.is_nan() {
        return false;
    }

    match (a, b) {
        (JsValue::Undefined, JsValue::Undefined) | (JsValue::Null, JsValue::Null) => true,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::String(x), JsValue::String(y)) => x == y,
        (JsValue::Number(x), JsValue::Bool(y)) | (JsValue::Bool(y), JsValue::Number(x)) => {
            let mut bit = 0.0;

            if *y {
                bit = 1.0;
            }

            *x == bit
        }
        (JsValue::Array(_), JsValue::Array(_))
        | (JsValue::Object(_), JsValue::Object(_))
        | (JsValue::Regex(_), JsValue::Regex(_)) => a.same_object(b),
        (JsValue::Function(x), JsValue::Function(y)) => x.repr() == y.repr(),
        _ => false,
    }
}

/// youtube-dl `_js_comp_op`. Undefined always false.
#[must_use]
pub fn js_cmp(a: &JsValue, b: &JsValue, op: CmpOp) -> bool {
    if matches!(a, JsValue::Undefined) || matches!(b, JsValue::Undefined) {
        return false;
    }

    if let (JsValue::String(sa), JsValue::String(sb)) = (a, b) {
        return match op {
            CmpOp::Lt => **sa < **sb,
            CmpOp::Le => **sa <= **sb,
            CmpOp::Gt => **sa > **sb,
            CmpOp::Ge => **sa >= **sb,
        };
    }

    if matches!(a, JsValue::String(_)) || matches!(b, JsValue::String(_)) {
        let sa = a.to_js_string();
        let sb = b.to_js_string();

        return match op {
            CmpOp::Lt => sa < sb,
            CmpOp::Le => sa <= sb,
            CmpOp::Gt => sa > sb,
            CmpOp::Ge => sa >= sb,
        };
    }

    let na = cmp_num(a);
    let nb = cmp_num(b);

    match op {
        CmpOp::Lt => na < nb,
        CmpOp::Le => na <= nb,
        CmpOp::Gt => na > nb,
        CmpOp::Ge => na >= nb,
    }
}

fn cmp_num(v: &JsValue) -> f64 {
    match v {
        JsValue::Null | JsValue::Bool(false) => 0.0,
        JsValue::Bool(true) => 1.0,
        JsValue::Number(n) => *n,
        JsValue::String(s) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

#[derive(Clone, Copy)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
}

/// youtube-dl `_js_bit_op` / `zeroise`.
#[must_use]
pub fn js_to_int32(v: &JsValue) -> i32 {
    let n = match v {
        JsValue::Number(n) => *n,
        JsValue::Bool(true) => 1.0,
        JsValue::Bool(false) | JsValue::Null | JsValue::Undefined => 0.0,
        JsValue::String(s) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
        _ => f64::NAN,
    };

    if !n.is_finite() {
        return 0;
    }

    let wrapped = n % 4_294_967_296.0;
    wrapped as i32
}

#[must_use]
pub fn js_shift_count(v: &JsValue) -> u32 {
    (js_to_int32(v) as u32) & 31
}

/// youtube-dl `_js_arith_op` (undefined → NaN; null/"" → 0).
#[must_use]
pub fn js_arith(a: &JsValue, b: &JsValue) -> (f64, f64) {
    if matches!(a, JsValue::Undefined) || matches!(b, JsValue::Undefined) {
        return (f64::NAN, f64::NAN);
    }

    (arith_one(a), arith_one(b))
}

fn arith_one(v: &JsValue) -> f64 {
    match v {
        JsValue::Null | JsValue::Bool(false) => 0.0,
        JsValue::Bool(true) => 1.0,
        JsValue::Number(n) => *n,
        JsValue::String(s) => parse_js_number(s),
        _ => f64::NAN,
    }
}

pub fn js_add(a: &JsValue, b: &JsValue) -> JsValue {
    if matches!(a, JsValue::String(_)) || matches!(b, JsValue::String(_)) {
        let mut out = a.to_js_string();
        out.push_str(&b.to_js_string());
        return JsValue::from_str(out);
    }

    let (x, y) = js_arith(a, b);
    JsValue::Number(x + y)
}

pub fn js_sub(a: &JsValue, b: &JsValue) -> JsValue {
    let (x, y) = js_arith(a, b);
    JsValue::Number(x - y)
}

pub fn js_mul(a: &JsValue, b: &JsValue) -> JsValue {
    let (x, y) = js_arith(a, b);
    JsValue::Number(x * y)
}

pub fn js_div(a: &JsValue, b: &JsValue) -> JsValue {
    if matches!(a, JsValue::Undefined) || matches!(b, JsValue::Undefined) {
        return JsValue::nan();
    }

    let (x, y) = js_arith(a, b);

    if y == 0.0 {
        if x == 0.0 {
            return JsValue::nan();
        }

        if x > 0.0 {
            return JsValue::infinity();
        }

        return JsValue::Number(f64::NEG_INFINITY);
    }

    JsValue::Number(x / y)
}

pub fn js_mod(a: &JsValue, b: &JsValue) -> JsValue {
    let (x, y) = js_arith(a, b);

    if y == 0.0 || x.is_nan() || y.is_nan() {
        return JsValue::nan();
    }

    JsValue::Number(x % y)
}

pub fn js_exp(a: &JsValue, b: &JsValue) -> JsValue {
    if py_falsy(b) {
        return JsValue::Number(1.0);
    }

    let (x, y) = js_arith(a, b);
    JsValue::Number(x.powf(y))
}

fn py_falsy(v: &JsValue) -> bool {
    match v {
        JsValue::Null | JsValue::Bool(false) => true,
        JsValue::Number(n) if *n == 0.0 => true,
        JsValue::String(s) if s.is_empty() => true,
        _ => false,
    }
}
