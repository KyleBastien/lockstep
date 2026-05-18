//! Peer test file for `elide.rs`.

use crate::elide::{strip, TsFlavor};

fn s(input: &str) -> String {
    strip(input, TsFlavor::Ts).unwrap().output
}

/// `(label, input, must_contain[], must_not_contain[])`.
type StripCase = (
    &'static str,
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

fn assert_case(case: &StripCase) {
    let (label, input, must_contain, must_not_contain) = case;
    let out = s(input);
    for needle in *must_contain {
        assert!(
            out.contains(needle),
            "[{label}] missing `{needle}` in: {out}"
        );
    }
    for forbidden in *must_not_contain {
        assert!(
            !out.contains(forbidden),
            "[{label}] unexpected `{forbidden}` in: {out}",
        );
    }
}

#[test]
fn drops_type_annotation_on_let() {
    assert_case(&(
        "type annotation",
        "let x: number = 1;",
        &["let x", "= 1"],
        &[":"],
    ));
}

#[test]
fn drops_interface_declaration() {
    assert_case(&(
        "interface",
        "interface Foo { x: number }\nlet a = 1;",
        &["let a = 1"],
        &["interface"],
    ));
}

#[test]
fn drops_type_alias() {
    assert_case(&(
        "type alias",
        "type Foo = number;\nlet a = 1;",
        &["let a = 1"],
        &["type Foo"],
    ));
}

#[test]
fn unwraps_as_expression() {
    assert_case(&(
        "as expression",
        "let a = (x as number) + 1;",
        &["x", "+ 1"],
        &["as", "number"],
    ));
}

#[test]
fn unwraps_non_null_assertion() {
    assert_case(&("non-null !", "let a = foo!.bar;", &["foo.bar"], &["!"]));
}

#[test]
fn drops_pure_type_import() {
    assert_case(&(
        "pure type import",
        "import type { Foo } from 'x';\nlet a = 1;",
        &["let a = 1"],
        &["Foo"],
    ));
}

#[test]
fn drops_generics_on_call() {
    assert_case(&(
        "call generics",
        "let a = foo<number, string>(1);",
        &["foo(1)"],
        &["<number"],
    ));
}

#[test]
fn drops_generic_function_signature() {
    assert_case(&(
        "generic fn sig",
        "function id<T>(x: T): T { return x; }",
        &["function id(x) { return x; }"],
        &["<T>", ": T"],
    ));
}

#[test]
fn enum_produces_rejection() {
    let r = strip("enum Color { Red, Green }", TsFlavor::Ts).unwrap();
    assert_eq!(r.rejections.len(), 1);
    assert_eq!(r.rejections[0].kind, "enum_declaration");
}

#[test]
fn parameter_property_produces_rejection() {
    let r = strip("class C { constructor(public x: number) {} }", TsFlavor::Ts).unwrap();
    assert!(r.rejections.iter().any(|r| r.kind == "parameter_property"));
}

#[test]
fn drops_accessibility_modifier() {
    assert_case(&(
        "private",
        "class C { private x = 1; foo() { return this.x; } }",
        &["x = 1"],
        &["private"],
    ));
}

#[test]
fn drops_readonly_modifier() {
    assert_case(&(
        "readonly",
        "class C { readonly x = 1; }",
        &["x = 1"],
        &["readonly"],
    ));
}

#[test]
fn unwraps_satisfies() {
    assert_case(&(
        "satisfies",
        "let a = ({ x: 1 } satisfies Foo);",
        &[],
        &["satisfies", "Foo"],
    ));
}

#[test]
fn tsflavor_from_extension_picks_tsx_only_for_tsx() {
    use std::path::Path;
    assert_eq!(TsFlavor::from_extension(Path::new("a.ts")), TsFlavor::Ts);
    assert_eq!(TsFlavor::from_extension(Path::new("a.tsx")), TsFlavor::Tsx);
    assert_eq!(
        TsFlavor::from_extension(Path::new("a.unknown")),
        TsFlavor::Ts
    );
}
