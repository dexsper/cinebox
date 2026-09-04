use super::*;

fn call(code: &str, func: &str, args: &[JsValue]) -> JsValue {
    let mut jsi = JSInterpreter::new(code);
    match jsi.call_function(func, args) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    }
}

fn assert_call(code: &str, expected: JsValue) {
    assert_eq!(call(code, "f", &[]), expected);
}

fn assert_call_args(code: &str, expected: JsValue, args: &[JsValue]) {
    assert_eq!(call(code, "f", args), expected);
}

fn assert_nan(val: &JsValue) {
    assert!(val.is_nan(), "{val:?} is not NaN");
}

fn num(n: f64) -> JsValue {
    JsValue::Number(n)
}

fn s(text: &str) -> JsValue {
    JsValue::from_str(text)
}

fn arr(items: Vec<JsValue>) -> JsValue {
    JsValue::array(items)
}

#[test]
fn test_basic() {
    let mut jsi = JSInterpreter::new("function f(){;}");
    let func = match jsi.extract_function("f") {
        Ok(func) => func,
        Err(err) => panic!("{err}"),
    };
    assert_eq!(func.to_string(), "F<f>");
    assert_eq!(call("function f(){;}", "f", &[]), JsValue::Null);

    assert_call("function f(){return 42;}", num(42.0));
    assert_call("function f(){42}", JsValue::Null);
    assert_call("var f = function(){return 42;}", num(42.0));
}

#[test]
fn test_add() {
    assert_call("function f(){return 42 + 7;}", num(49.0));
    assert_nan(&call("function f(){return 42 + undefined;}", "f", &[]));
    assert_call("function f(){return 42 + null;}", num(42.0));
    assert_call("function f(){return 1 + \"\";}", s("1"));
    assert_call("function f(){return 42 + \"7\";}", s("427"));
    assert_call("function f(){return false + true;}", num(1.0));
    assert_call("function f(){return \"false\" + true;}", s("falsetrue"));
    assert_call(
        "function f(){return 1 + \"2\" + [3,4] + {k: 56} + null + undefined + Infinity;}",
        s("123,4[object Object]nullundefinedInfinity"),
    );
}

#[test]
fn test_sub() {
    assert_call("function f(){return 42 - 7;}", num(35.0));
    assert_nan(&call("function f(){return 42 - undefined;}", "f", &[]));
    assert_call("function f(){return 42 - null;}", num(42.0));
    assert_call("function f(){return 42 - \"7\";}", num(35.0));
    assert_nan(&call("function f(){return 42 - \"spam\";}", "f", &[]));
}

#[test]
fn test_mul() {
    assert_call("function f(){return 42 * 7;}", num(294.0));
    assert_nan(&call("function f(){return 42 * undefined;}", "f", &[]));
    assert_call("function f(){return 42 * null;}", num(0.0));
    assert_call("function f(){return 42 * \"7\";}", num(294.0));
    assert_nan(&call("function f(){return 42 * \"eggs\";}", "f", &[]));
}

#[test]
fn test_div() {
    let code = "function f(a, b){return a / b;}";
    assert_nan(&call(code, "f", &[num(0.0), num(0.0)]));
    assert_nan(&call(code, "f", &[JsValue::Undefined, num(1.0)]));
    assert_call_args(
        code,
        JsValue::infinity(),
        &[num(2.0), num(0.0)],
    );
    assert_eq!(call(code, "f", &[num(0.0), num(3.0)]), num(0.0));
    assert_eq!(call(code, "f", &[num(42.0), num(7.0)]), num(6.0));
    assert_eq!(
        call(code, "f", &[num(42.0), JsValue::infinity()]),
        num(0.0)
    );
    assert_eq!(call(code, "f", &[s("42"), num(7.0)]), num(6.0));
    assert_nan(&call(code, "f", &[s("spam"), num(7.0)]));
}

#[test]
fn test_mod() {
    assert_call("function f(){return 42 % 7;}", num(0.0));
    assert_nan(&call("function f(){return 42 % 0;}", "f", &[]));
    assert_nan(&call("function f(){return 42 % undefined;}", "f", &[]));
    assert_call("function f(){return 42 % \"7\";}", num(0.0));
    assert_nan(&call("function f(){return 42 % \"beans\";}", "f", &[]));
}

#[test]
fn test_exp() {
    assert_call("function f(){return 42 ** 2;}", num(1764.0));
    assert_nan(&call("function f(){return 42 ** undefined;}", "f", &[]));
    assert_call("function f(){return 42 ** null;}", num(1.0));
    assert_call("function f(){return undefined ** 0;}", num(1.0));
    assert_nan(&call("function f(){return undefined ** 42;}", "f", &[]));
    assert_call("function f(){return 42 ** \"2\";}", num(1764.0));
    assert_nan(&call("function f(){return 42 ** \"spam\";}", "f", &[]));
}

#[test]
fn test_calc() {
    assert_eq!(
        call("function f(a){return 2*a+1;}", "f", &[num(3.0)]),
        num(7.0)
    );
}

#[test]
fn test_empty_return() {
    assert_call("function f(){return; y()}", JsValue::Null);
}

#[test]
fn test_morespace() {
    assert_eq!(
        call("function f (a) { return 2 * a + 1 ; }", "f", &[num(3.0)]),
        num(7.0)
    );
    assert_call("function f () { x =  2  ; return x; }", num(2.0));
}

#[test]
fn test_strange_chars() {
    assert_eq!(
        call(
            "function $_xY1 ($_axY1) { var $_axY2 = $_axY1 + 1; return $_axY2; }",
            "$_xY1",
            &[num(20.0)],
        ),
        num(21.0)
    );
}

#[test]
fn test_operators() {
    assert_call("function f(){return 1 << 5;}", num(32.0));
    assert_call("function f(){return 2 ** 5}", num(32.0));
    assert_call("function f(){return 19 & 21;}", num(17.0));
    assert_call("function f(){return 11 >> 2;}", num(2.0));
    assert_call("function f(){return []? 2+3: 4;}", num(5.0));
    assert_call("function f(){return 1 == 1}", JsValue::Bool(true));
    assert_call("function f(){return 1 == 1.0}", JsValue::Bool(true));
    assert_call("function f(){return 1 == \"1\"}", JsValue::Bool(true));
    assert_call("function f(){return 1 == 2}", JsValue::Bool(false));
    assert_call("function f(){return 1 != \"1\"}", JsValue::Bool(false));
    assert_call("function f(){return 1 != 2}", JsValue::Bool(true));
    assert_call(
        "function f(){var x = {a: 1}; var y = x; return x == y}",
        JsValue::Bool(true),
    );
    assert_call(
        "function f(){var x = {a: 1}; return x == {a: 1}}",
        JsValue::Bool(false),
    );
    assert_call("function f(){return NaN == NaN}", JsValue::Bool(false));
    assert_call(
        "function f(){return null == undefined}",
        JsValue::Bool(true),
    );
    assert_call(
        "function f(){return \"spam, eggs\" == \"spam, eggs\"}",
        JsValue::Bool(true),
    );
    assert_call("function f(){return 1 === 1}", JsValue::Bool(true));
    assert_call("function f(){return 1 === 1.0}", JsValue::Bool(true));
    assert_call("function f(){return 1 === \"1\"}", JsValue::Bool(false));
    assert_call("function f(){return 1 === 2}", JsValue::Bool(false));
    assert_call(
        "function f(){var x = {a: 1}; var y = x; return x === y}",
        JsValue::Bool(true),
    );
    assert_call(
        "function f(){var x = {a: 1}; return x === {a: 1}}",
        JsValue::Bool(false),
    );
    assert_call("function f(){return NaN === NaN}", JsValue::Bool(false));
    assert_call(
        "function f(){return null === undefined}",
        JsValue::Bool(false),
    );
    assert_call("function f(){return null === null}", JsValue::Bool(true));
    assert_call(
        "function f(){return undefined === undefined}",
        JsValue::Bool(true),
    );
    assert_call(
        "function f(){return \"uninterned\" === \"uninterned\"}",
        JsValue::Bool(true),
    );
    assert_call("function f(){return 1 === 1}", JsValue::Bool(true));
    assert_call("function f(){return 1 === \"1\"}", JsValue::Bool(false));
    assert_call("function f(){return 1 !== 1}", JsValue::Bool(false));
    assert_call("function f(){return 1 !== \"1\"}", JsValue::Bool(true));
    assert_call("function f(){return 0 && 1 || 2;}", num(2.0));
    assert_call("function f(){return 0 ?? 42;}", num(0.0));
    assert_call(
        "function f(){return \"life, the universe and everything\" < 42;}",
        JsValue::Bool(false),
    );
    assert_call("function f(){return 0  - 7 * - 6;}", num(42.0));
}

#[test]
fn test_bitwise_operators_typecast() {
    assert_call("function f(){return null << 5}", num(0.0));
    assert_call("function f(){return undefined >> 5}", num(0.0));
    assert_call("function f(){return 42 << NaN}", num(42.0));
    assert_call("function f(){return 42 << Infinity}", num(42.0));
    assert_call("function f(){return 0.0 << null}", num(0.0));
    assert_call("function f(){return NaN << 42}", num(0.0));
    assert_call("function f(){return \"21.9\" << 1}", num(42.0));
    assert_call("function f(){return true << \"5\";}", num(32.0));
    assert_call("function f(){return true << true;}", num(2.0));
    assert_call("function f(){return \"19\" & \"21.9\";}", num(17.0));
    assert_call("function f(){return \"19\" & false;}", num(0.0));
    assert_call("function f(){return \"11.0\" >> \"2.1\";}", num(2.0));
    assert_call("function f(){return 5 ^ 9;}", num(12.0));
    assert_call("function f(){return 0.0 << NaN}", num(0.0));
    assert_call("function f(){return null << undefined}", num(0.0));
    assert_call("function f(){return 21 << 4294967297}", num(42.0));
}

#[test]
fn test_array_access() {
    assert_call(
        "function f(){var x = [1,2,3]; x[0] = 4; x[0] = 5; x[2.0] = 7; return x;}",
        arr(vec![num(5.0), num(2.0), num(7.0)]),
    );
}

#[test]
fn test_parens() {
    assert_call(
        "function f(){return (1) + (2) * ((( (( (((((3)))))) )) ));}",
        num(7.0),
    );
    assert_call("function f(){return (1 + 2) * 3;}", num(9.0));
}

#[test]
fn test_quotes() {
    assert_call(r#"function f(){return "a\"\\("}"#, s(r#"a"\("#));
}

#[test]
fn test_assignments() {
    assert_call(
        "function f(){var x = 20; x = 30 + 1; return x;}",
        num(31.0),
    );
    assert_call(
        "function f(){var x = 20; x += 30 + 1; return x;}",
        num(51.0),
    );
    assert_call(
        "function f(){var x = 20; x -= 30 + 1; return x;}",
        num(-11.0),
    );
    assert_call(
        r#"function f(){var x = 2; var y = ["a", "b"]; y[x%y["length"]]="z"; return y}"#,
        arr(vec![s("z"), s("b")]),
    );
}

#[test]
fn test_comments() {
    assert_call(
        r"
            function f() {
                var x = /* 1 + */ 2;
                var y = /* 30
                * 40 */ 50;
                return x + y;
            }
        ",
        num(52.0),
    );
    assert_call(
        r#"
            function f() {
                var x = "/*";
                var y = 1 /* comment */ + 2;
                return y;
            }
        "#,
        num(3.0),
    );
    assert_call(
        r"
            function f() {
                var x = ( /* 1 + */ 2 +
                          /* 30 * 40 */
                          50);
                return x;
            }
        ",
        num(52.0),
    );
}

#[test]
fn test_precedence() {
    assert_call(
        r"
            function f() {
                var a = [10, 20, 30, 40, 50];
                var b = 6;
                a[0]=a[b%a.length];
                return a;
            }
        ",
        arr(vec![num(20.0), num(20.0), num(30.0), num(40.0), num(50.0)]),
    );
}

#[test]
fn test_builtins() {
    assert_nan(&call("function f() { return NaN }", "f", &[]));
}

#[test]
#[allow(non_snake_case)]
fn test_Date() {
    assert_call(
        r#"function f() { return new Date("Wednesday 31 December 1969 18:01:26 MDT") - 0; }"#,
        num(86000.0),
    );

    let parse = "function f(dt) { return new Date(dt) - 0; }";
    assert_eq!(
        call(parse, "f", &[s("12/31/1969 18:01:26 MDT")]),
        num(86000.0)
    );
    assert_eq!(
        call(parse, "f", &[s("1 January 1970 00:00:00 UTC")]),
        num(0.0)
    );
    assert_nan(&call(parse, "f", &[JsValue::Undefined]));

    let local = call(
        "function f() { return new Date(2024, 5, 29, 2, 52, 12, 42); }",
        "f",
        &[],
    );
    let expected_local = local_date_ms(2024, 6, 29, 2, 52, 12, 42);
    match local {
        JsValue::Number(n) => assert!((n - expected_local).abs() < 1.0, "{n} vs {expected_local}"),
        other => panic!("expected number, got {other:?}"),
    }

    let now = call("function f() { return new Date() - 0; }", "f", &[]);
    assert_almost_now(&now);

    let now = call("function f() { return Date.now(); }", "f", &[]);
    assert_almost_now(&now);

    let parse_fn = "function f(dt) { return Date.parse(dt); }";
    assert_eq!(
        call(parse_fn, "f", &[s("1 January 1970 00:00:00 UTC")]),
        num(0.0)
    );

    assert_call(
        "function f() { return Date.UTC(1970, 0, 1, 0, 0, 0, 0); }",
        num(0.0),
    );
}

fn local_date_ms(y: i32, month: u32, day: u32, h: u32, min: u32, sec: u32, ms: u32) -> f64 {
    use chrono::{Local, NaiveDate, TimeZone};

    let Some(date) = NaiveDate::from_ymd_opt(y, month, day) else {
        panic!("invalid date");
    };
    let Some(naive) = date.and_hms_milli_opt(h, min, sec, ms) else {
        panic!("invalid time");
    };

    match Local.from_local_datetime(&naive) {
        chrono::LocalResult::Single(dt) | chrono::LocalResult::Ambiguous(dt, _) => {
            dt.timestamp_millis() as f64
        }
        chrono::LocalResult::None => panic!("unmapped local datetime"),
    }
}

fn assert_almost_now(val: &JsValue) {
    let JsValue::Number(n) = val else {
        panic!("expected number, got {val:?}");
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(0.0);
    assert!((n - now).abs() < 100.0, "{n} not within 100ms of {now}");
}

#[test]
fn test_call() {
    let code = r"
        function x() { return 2; }
        function y(a) { return x() + (a?a:0); }
        function z() { return y(3); }
        ";
    assert_eq!(call(code, "z", &[]), num(5.0));
    assert_eq!(call(code, "y", &[]), num(2.0));
}

#[test]
fn test_if() {
    assert_call(
        r"
            function f() {
            let a = 9;
            if (0==0) {a++}
            return a
            }
        ",
        num(10.0),
    );
    assert_call(
        r"
            function f() {
            if (0==0) {return 10}
            }
        ",
        num(10.0),
    );
    assert_call(
        r"
            function f() {
            if (0!=0) {return 1}
            else {return 10}
            }
        ",
        num(10.0),
    );
}

#[test]
fn test_elseif() {
    assert_call(
        r"
            function f() {
                if (0!=0) {return 1}
                else if (1==0) {return 2}
                else {return 10}
            }
        ",
        num(10.0),
    );
}

#[test]
fn test_for_loop() {
    assert_call(
        "function f() { a=0; for (i=0; i-10; i++) {a++} return a }",
        num(10.0),
    );
}

#[test]
fn test_while_loop() {
    assert_call(
        "function f() { a=0; while (a<10) {a++} return a }",
        num(10.0),
    );
}

#[test]
fn test_switch() {
    let code = r"
            function f(x) { switch(x){
                case 1:x+=1;
                case 2:x+=2;
                case 3:x+=3;break;
                case 4:x+=4;
                default:x=0;
            } return x }
        ";
    assert_eq!(call(code, "f", &[num(1.0)]), num(7.0));
    assert_eq!(call(code, "f", &[num(3.0)]), num(6.0));
    assert_eq!(call(code, "f", &[num(5.0)]), num(0.0));
}

#[test]
fn test_switch_default() {
    let code = r"
            function f(x) { switch(x){
                case 2: x+=2;
                default: x-=1;
                case 5:
                case 6: x+=6;
                case 0: break;
                case 1: x+=1;
            } return x }
        ";
    assert_eq!(call(code, "f", &[num(1.0)]), num(2.0));
    assert_eq!(call(code, "f", &[num(5.0)]), num(11.0));
    assert_eq!(call(code, "f", &[num(9.0)]), num(14.0));
}

#[test]
fn test_try() {
    assert_call(
        "function f() { try{return 10} catch(e){return 5} }",
        num(10.0),
    );
}

#[test]
fn test_catch() {
    assert_call(
        "function f() { try{throw 10} catch(e){return 5} }",
        num(5.0),
    );
}

#[test]
fn test_finally() {
    assert_call(
        "function f() { try{throw 10} finally {return 42} }",
        num(42.0),
    );
    assert_call(
        "function f() { try{throw 10} catch(e){return 5} finally {return 42} }",
        num(42.0),
    );
}

#[test]
fn test_nested_try() {
    assert_call(
        r"
            function f() {try {
                try{throw 10} finally {throw 42}
            } catch(e){return 5} }
        ",
        num(5.0),
    );
}

#[test]
fn test_for_loop_continue() {
    assert_call(
        "function f() { a=0; for (i=0; i-10; i++) { continue; a++ } return a }",
        num(0.0),
    );
}

#[test]
fn test_for_loop_break() {
    assert_call(
        "function f() { a=0; for (i=0; i-10; i++) { break; a++ } return a }",
        num(0.0),
    );
}

#[test]
fn test_for_loop_try() {
    assert_call(
        r"
            function f() {
                for (i=0; i-10; i++) { try { if (i == 5) throw i} catch {return 10} finally {break} };
                return 42 }
        ",
        num(42.0),
    );
}

#[test]
fn test_literal_list() {
    assert_call(
        r#"function f() { return [1, 2, "asdf", [5, 6, 7]][3] }"#,
        arr(vec![num(5.0), num(6.0), num(7.0)]),
    );
}

#[test]
fn test_comma() {
    assert_call("function f() { a=5; a -= 1, a+=3; return a }", num(7.0));
    assert_call(
        "function f() { a=5; return (a -= 1, a+=3, a); }",
        num(7.0),
    );
    assert_call(
        "function f() { return (l=[0,1,2,3], function(a, b){return a+b})((l[1], l[2]), l[3]) }",
        num(5.0),
    );
}

#[test]
fn test_not() {
    assert_call("function f() { return ! undefined; }", JsValue::Bool(true));
    assert_call("function f() { return !0; }", JsValue::Bool(true));
    assert_call("function f() { return !!0; }", JsValue::Bool(false));
    assert_call("function f() { return ![]; }", JsValue::Bool(false));
    assert_call(
        "function f() { return !0 !== false; }",
        JsValue::Bool(true),
    );
}

#[test]
fn test_void() {
    assert_call("function f() { return void 42; }", JsValue::Undefined);
}

#[test]
fn test_typeof() {
    assert_call(
        "function f() { return typeof undefined; }",
        s("undefined"),
    );
    assert_call("function f() { return typeof NaN; }", s("number"));
    assert_call("function f() { return typeof Infinity; }", s("number"));
    assert_call("function f() { return typeof true; }", s("boolean"));
    assert_call("function f() { return typeof null; }", s("object"));
    assert_call(
        "function f() { return typeof \"a string\"; }",
        s("string"),
    );
    assert_call("function f() { return typeof 42; }", s("number"));
    assert_call("function f() { return typeof 42.42; }", s("number"));
    assert_call(
        "function f() { var g = function(){}; return typeof g; }",
        s("function"),
    );
    assert_call(
        r#"function f() { return typeof {key: "value"}; }"#,
        s("object"),
    );
}

#[test]
fn test_return_function() {
    let mut jsi = JSInterpreter::new(
        r"
        function x() { return [1, function(){return 1}][1] }
        ",
    );
    let inner = match jsi.call_function("x", &[]) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    };
    let JsValue::Function(func) = inner else {
        panic!("expected function, got {inner:?}");
    };
    match func.call(&[]) {
        Ok(val) => assert_eq!(val, num(1.0)),
        Err(err) => panic!("{err}"),
    }
}

#[test]
fn test_null() {
    assert_call("function f() { return null; }", JsValue::Null);
    assert_call(
        "function f() { return [null > 0, null < 0, null == 0, null === 0]; }",
        arr(vec![
            JsValue::Bool(false),
            JsValue::Bool(false),
            JsValue::Bool(false),
            JsValue::Bool(false),
        ]),
    );
    assert_call(
        "function f() { return [null >= 0, null <= 0]; }",
        arr(vec![JsValue::Bool(true), JsValue::Bool(true)]),
    );
}

#[test]
fn test_undefined() {
    assert_call(
        "function f() { return undefined === undefined; }",
        JsValue::Bool(true),
    );
    assert_call("function f() { return undefined; }", JsValue::Undefined);
    assert_call("function f() { return undefined ?? 42; }", num(42.0));
    assert_call("function f() { let v; return v; }", JsValue::Undefined);
    assert_call("function f() { let v; return v**0; }", num(1.0));
    assert_call(
        "function f() { let v; return [v>42, v<=42, v&&42, 42&&v]; }",
        arr(vec![
            JsValue::Bool(false),
            JsValue::Bool(false),
            JsValue::Undefined,
            JsValue::Undefined,
        ]),
    );
    assert_call(
        r"
            function f() { return [
                undefined === undefined,
                undefined == undefined,
                undefined == null
            ]; }
        ",
        arr(vec![
            JsValue::Bool(true),
            JsValue::Bool(true),
            JsValue::Bool(true),
        ]),
    );
    assert_call(
        r"
            function f() { return [
                undefined < undefined,
                undefined > undefined,
                undefined === 0,
                undefined == 0,
                undefined < 0,
                undefined > 0,
                undefined >= 0,
                undefined <= 0,
                undefined > null,
                undefined < null,
                undefined === null
            ]; }
        ",
        arr(vec![JsValue::Bool(false); 11]),
    );

    let mut jsi = JSInterpreter::new(
        r"
            function x() { let v; return [42+v, v+42, v**42, 42**v, 0**v]; }
        ",
    );
    let got = match jsi.call_function("x", &[]) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    };
    let JsValue::Array(items) = got else {
        panic!("expected array");
    };
    for y in items.borrow().iter() {
        assert_nan(y);
    }
}

#[test]
fn test_object() {
    assert_call("function f() { return {}; }", JsValue::object(Default::default()));
    assert_call(
        "function f() { let a = {m1: 42, m2: 0 }; return [a[\"m1\"], a.m2]; }",
        arr(vec![num(42.0), num(0.0)]),
    );
    assert_call(
        "function f() { let a; return a?.qq; }",
        JsValue::Undefined,
    );
    assert_call(
        "function f() { let a = {m1: 42, m2: 0 }; return a?.qq; }",
        JsValue::Undefined,
    );
}

#[test]
fn test_indexing() {
    assert_call("function f() { return [1, 2, 3, 4][3]}", num(4.0));
    assert_call(
        "function f() { return [1, [2, [3, [4]]]][1][1][1][0]}",
        num(4.0),
    );
    assert_call(
        "function f() { var o = {1: 2, 3: 4}; return o[3]}",
        num(4.0),
    );
    assert_call(
        r#"function f() { var o = {1: 2, 3: 4}; return o["3"]}"#,
        num(4.0),
    );
    assert_call(
        r#"function f() { return [1, [2, {3: [4]}]][1][1]["3"][0]}"#,
        num(4.0),
    );
    assert_call("function f() { return [1, 2, 3, 4].length}", num(4.0));
    assert_call(
        "function f() { var o = {1: 2, 3: 4}; return o.length}",
        JsValue::Undefined,
    );
    assert_call(
        r#"function f() { var o = {1: 2, 3: 4}; o["length"] = 42; return o.length}"#,
        num(42.0),
    );
}

#[test]
fn test_regex() {
    assert_call("function f() { let a=/,,[/,913,/](,)}/; }", JsValue::Null);
    assert_call(
        "function f() { let a=/,,[/,913,/](,)}/; return a.source;  }",
        s(",,[/,913,/](,)}"),
    );

    let mut jsi = JSInterpreter::new(
        r#"
            function x() { let a=/,,[/,913,/](,)}/; "".replace(a, ""); return a; }
        "#,
    );
    let got = match jsi.call_function("x", &[]) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    };
    let JsValue::Regex(re) = got else {
        panic!("expected regex, got {got:?}");
    };
    assert_eq!(re.pattern(), ",,[/,913,/](,)}");

    let mut jsi = JSInterpreter::new(
        r"
            function x() { let a=/,,[/,913,/](,)}/i; return a; }
        ",
    );
    let got = match jsi.call_function("x", &[]) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    };
    let JsValue::Regex(re) = got else {
        panic!("expected regex");
    };
    assert!(re.ignore_case());

    let mut jsi = JSInterpreter::new(r#"function f() { let a=/,][}",],()}(\[)/; return a; }"#);
    let got = match jsi.call_function("f", &[]) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    };
    let JsValue::Regex(re) = got else {
        panic!("expected regex");
    };
    assert_eq!(re.pattern(), r#",][}",],()}(\[)"#);

    let mut jsi = JSInterpreter::new(r"function f() { let a=[/[)\\]/]; return a[0]; }");
    let got = match jsi.call_function("f", &[]) {
        Ok(val) => val,
        Err(err) => panic!("{err}"),
    };
    let JsValue::Regex(re) = got else {
        panic!("expected regex");
    };
    assert_eq!(re.pattern(), r"[)\\]");
}

#[test]
fn test_replace() {
    assert_call(
        r#"function f() { let a="data-name".replace("data-", ""); return a }"#,
        s("name"),
    );
    assert_call(
        r#"function f() { let a="data-name".replace(new RegExp("^.+-"), ""); return a; }"#,
        s("name"),
    );
    assert_call(
        r#"function f() { let a="data-name".replace(/^.+-/, ""); return a; }"#,
        s("name"),
    );
    assert_call(
        r#"function f() { let a="data-name".replace(/a/g, "o"); return a; }"#,
        s("doto-nome"),
    );
    assert_call(
        r#"function f() { let a="data-name".replaceAll("a", "o"); return a; }"#,
        s("doto-nome"),
    );
}

#[test]
fn test_char_code_at() {
    let code = r#"function f(i){return "test".charCodeAt(i)}"#;
    assert_eq!(call(code, "f", &[num(0.0)]), num(116.0));
    assert_eq!(call(code, "f", &[num(1.0)]), num(101.0));
    assert_eq!(call(code, "f", &[num(2.0)]), num(115.0));
    assert_eq!(call(code, "f", &[num(3.0)]), num(116.0));
    assert_eq!(call(code, "f", &[num(4.0)]), JsValue::Null);
    assert_eq!(call(code, "f", &[s("not_a_number")]), num(116.0));
}

#[test]
fn test_bitwise_operators_overflow() {
    assert_call("function f(){return -524999584 << 5}", num(379_882_496.0));
    assert_call("function f(){return 1236566549 << 5}", num(915_423_904.0));
}

#[test]
fn test_negative() {
    assert_call("function f(){return 2    *    -2.0    ;}", num(-4.0));
    assert_call("function f(){return 2    -    - -2    ;}", num(0.0));
    assert_call("function f(){return 2    -    - - -2  ;}", num(4.0));
    assert_call("function f(){return 2    -    + + - -2;}", num(0.0));
    assert_call("function f(){return 2    +    - + - -2;}", num(0.0));
}

#[test]
fn test_32066() {
    assert_call(
        "function f(){return Math.pow(3, 5) + new Date('1970-01-01T08:01:42.000+08:00') / 1000 * -239 - -24205;}",
        num(70.0),
    );
}

#[test]
fn test_join() {
    let test_input = arr(vec![s("t"), s("e"), s("s"), s("t")]);
    let tests = [
        "function f(a, b){return a.join(b)}",
        "function f(a, b){return Array.prototype.join.call(a, b)}",
        "function f(a, b){return Array.prototype.join.apply(a, [b])}",
    ];
    for test in tests {
        assert_eq!(call(test, "f", &[test_input.clone(), s("")]), s("test"));
        assert_eq!(
            call(test, "f", &[test_input.clone(), s("-")]),
            s("t-e-s-t")
        );
        assert_eq!(call(test, "f", &[arr(Vec::new()), s("-")]), s(""));
    }

    assert_call(
        r#"function f(){return [1, 1.0, "abc", {a: 1}, null, undefined, Infinity, NaN].join()}"#,
        s("1,1,abc,[object Object],,,Infinity,NaN"),
    );
    assert_call(
        r#"function f(){return [1, 1.0, "abc", {a: 1}, null, undefined, Infinity, NaN].join("~")}"#,
        s("1~1~abc~[object Object]~~~Infinity~NaN"),
    );
}

#[test]
fn test_split() {
    let expected = arr(vec![s("t"), s("e"), s("s"), s("t")]);
    let tests = [
        "function f(a, b){return a.split(b)}",
        r#"function f(a, b){return a["split"](b)}"#,
        r#"function f(a, b){let x = ["split"]; return a[x[0]](b)}"#,
        "function f(a, b){return String.prototype.split.call(a, b)}",
        "function f(a, b){return String.prototype.split.apply(a, [b])}",
    ];
    for test in tests {
        assert_eq!(call(test, "f", &[s("test"), s("")]), expected);
        assert_eq!(call(test, "f", &[s("t-e-s-t"), s("-")]), expected);
        assert_eq!(call(test, "f", &[s(""), s("-")]), arr(vec![s("")]));
        assert_eq!(call(test, "f", &[s(""), s("")]), arr(Vec::new()));
    }

    assert_call(
        r#"function f(){return "test".split(/(?:)/)}"#,
        expected,
    );
    assert_call(
        r#"function f(){return "t-e-s-t".split(/[es-]+/)}"#,
        arr(vec![s("t"), s("t")]),
    );
    assert_call(
        r#"function f(){return "😄😄".split(/(?:)/u)}"#,
        arr(vec![s("😄"), s("😄")]),
    );
}

#[test]
fn test_slice() {
    let full = arr((0..9).map(|n| num(f64::from(n))).collect());
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice()}",
        full.clone(),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(0)}",
        full.clone(),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(5)}",
        arr(vec![num(5.0), num(6.0), num(7.0), num(8.0)]),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(99)}",
        arr(Vec::new()),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(-2)}",
        arr(vec![num(7.0), num(8.0)]),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(-99)}",
        full,
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(0, 0)}",
        arr(Vec::new()),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(1, 0)}",
        arr(Vec::new()),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(0, 1)}",
        arr(vec![num(0.0)]),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(3, 6)}",
        arr(vec![num(3.0), num(4.0), num(5.0)]),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(1, -1)}",
        arr(vec![
            num(1.0),
            num(2.0),
            num(3.0),
            num(4.0),
            num(5.0),
            num(6.0),
            num(7.0),
        ]),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(-1, 1)}",
        arr(Vec::new()),
    );
    assert_call(
        "function f(){return [0, 1, 2, 3, 4, 5, 6, 7, 8].slice(-3, -1)}",
        arr(vec![num(6.0), num(7.0)]),
    );
    assert_call(
        r#"function f(){return "012345678".slice()}"#,
        s("012345678"),
    );
    assert_call(
        r#"function f(){return "012345678".slice(0)}"#,
        s("012345678"),
    );
    assert_call(r#"function f(){return "012345678".slice(5)}"#, s("5678"));
    assert_call(r#"function f(){return "012345678".slice(99)}"#, s(""));
    assert_call(r#"function f(){return "012345678".slice(-2)}"#, s("78"));
    assert_call(
        r#"function f(){return "012345678".slice(-99)}"#,
        s("012345678"),
    );
    assert_call(r#"function f(){return "012345678".slice(0, 0)}"#, s(""));
    assert_call(r#"function f(){return "012345678".slice(1, 0)}"#, s(""));
    assert_call(r#"function f(){return "012345678".slice(0, 1)}"#, s("0"));
    assert_call(r#"function f(){return "012345678".slice(3, 6)}"#, s("345"));
    assert_call(
        r#"function f(){return "012345678".slice(1, -1)}"#,
        s("1234567"),
    );
    assert_call(r#"function f(){return "012345678".slice(-1, 1)}"#, s(""));
    assert_call(
        r#"function f(){return "012345678".slice(-3, -1)}"#,
        s("67"),
    );
}

#[test]
fn test_splice() {
    assert_call(
        r#"function f(){var T = ["0", "1", "2"]; T["splice"](2, 1, "0")[0]; return T }"#,
        arr(vec![s("0"), s("1"), s("0")]),
    );
}

#[test]
fn test_pop() {
    assert_call(
        "function f(){var a = [0, 1, 2, 3, 4, 5, 6, 7, 8]; return [a.pop(), a]}",
        arr(vec![
            num(8.0),
            arr(vec![
                num(0.0),
                num(1.0),
                num(2.0),
                num(3.0),
                num(4.0),
                num(5.0),
                num(6.0),
                num(7.0),
            ]),
        ]),
    );
    assert_call("function f(){return [].pop()}", JsValue::Undefined);
    assert_call(
        "function f(){var a = [0, 1, 2]; return [a.push(3, 4), a]}",
        arr(vec![
            num(5.0),
            arr(vec![num(0.0), num(1.0), num(2.0), num(3.0), num(4.0)]),
        ]),
    );
    assert_call(
        "function f(){var a = [0, 1, 2]; return [a.push(), a]}",
        arr(vec![num(3.0), arr(vec![num(0.0), num(1.0), num(2.0)])]),
    );
}

#[test]
fn test_shift() {
    assert_call(
        "function f(){var a = [0, 1, 2, 3, 4, 5, 6, 7, 8]; return [a.shift(), a]}",
        arr(vec![
            num(0.0),
            arr(vec![
                num(1.0),
                num(2.0),
                num(3.0),
                num(4.0),
                num(5.0),
                num(6.0),
                num(7.0),
                num(8.0),
            ]),
        ]),
    );
    assert_call("function f(){return [].shift()}", JsValue::Undefined);
    assert_call(
        "function f(){var a = [0, 1, 2]; return [a.unshift(3, 4), a]}",
        arr(vec![
            num(5.0),
            arr(vec![num(3.0), num(4.0), num(0.0), num(1.0), num(2.0)]),
        ]),
    );
    assert_call(
        "function f(){var a = [0, 1, 2]; return [a.unshift(), a]}",
        arr(vec![num(3.0), arr(vec![num(0.0), num(1.0), num(2.0)])]),
    );
}

#[test]
#[allow(non_snake_case)]
fn test_forEach() {
    assert_call(
        "function f(){var ret = []; var l = [4, 2]; var log = function(e,i,a){ret.push([e,i,a]);}; l.forEach(log); return [ret.length, ret[0][0], ret[1][1], ret[0][2]]}",
        arr(vec![
            num(2.0),
            num(4.0),
            num(1.0),
            arr(vec![num(4.0), num(2.0)]),
        ]),
    );
    assert_call(
        "function f(){var ret = []; var l = [4, 2]; var log = function(e,i,a){this.push([e,i,a]);}; l.forEach(log, ret); return [ret.length, ret[0][0], ret[1][1], ret[0][2]]}",
        arr(vec![
            num(2.0),
            num(4.0),
            num(1.0),
            arr(vec![num(4.0), num(2.0)]),
        ]),
    );
}

#[test]
fn test_extract_function() {
    let mut jsi = JSInterpreter::new("function a(b) { return b + 1; }");
    let func = match jsi.extract_function("a") {
        Ok(func) => func,
        Err(err) => panic!("{err}"),
    };
    match func.call(&[num(2.0)]) {
        Ok(val) => assert_eq!(val, num(3.0)),
        Err(err) => panic!("{err}"),
    }
}

#[test]
fn test_extract_function_with_global_stack() {
    let mut jsi = JSInterpreter::new("function c(d) { return d + e + f + g; }");
    let mut e = std::collections::HashMap::new();
    e.insert(String::from("e"), num(10.0));
    let mut fg = std::collections::HashMap::new();
    fg.insert(String::from("f"), num(100.0));
    fg.insert(String::from("g"), num(1000.0));
    let func = match jsi.extract_function_with("c", vec![e, fg]) {
        Ok(func) => func,
        Err(err) => panic!("{err}"),
    };
    match func.call(&[num(1.0)]) {
        Ok(val) => assert_eq!(val, num(1111.0)),
        Err(err) => panic!("{err}"),
    }
}
