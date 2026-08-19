//! Tests for `name := value` constants: parse-time substitution into bodies
//! and dependency arguments, param precedence, and the literal-only rules.

#[path = "../src/parser.rs"]
mod parser;

fn vf(src: &str) -> parser::Taskfile {
    parser::parse(src).expect("parse ok")
}

fn body_of(v: &parser::Taskfile, name: &str) -> String {
    v.tasks.get(name).expect("task exists").body.join("\n")
}

#[test]
fn constant_substitutes_in_body() {
    let v = vf(r#"
version := 1.4.2

build:
    echo building {{version}}
"#);
    assert_eq!(body_of(&v, "build"), "echo building 1.4.2");
}

#[test]
fn constant_defined_after_use_still_applies() {
    let v = vf(r#"
build:
    echo {{version}}

version := 2.0
"#);
    assert_eq!(body_of(&v, "build"), "echo 2.0");
}

#[test]
fn constant_substitutes_in_dep_args() {
    let v = vf(r#"
arch := arm64

build target:
    echo build {{target}}

all: build {{arch}}
"#);
    let all = v.tasks.get("all").expect("task exists");
    assert_eq!(all.deps[0].args, vec!["arm64".to_string()]);
}

#[test]
fn param_shadows_constant_in_body_and_dep_args() {
    let v = vf(r#"
name := global

greet name:
    echo hello {{name}}

send name: greet {{name}}
    echo sent to {{name}}
"#);
    // Body of a task whose param shares the name: left for run-time binding.
    assert_eq!(body_of(&v, "greet"), "echo hello {{name}}");
    // Dep arg referencing the declaring task's param: left for plan-time forwarding.
    let send = v.tasks.get("send").expect("task exists");
    assert_eq!(send.deps[0].args, vec!["{{name}}".to_string()]);
    assert_eq!(body_of(&v, "send"), "echo sent to {{name}}");
}

#[test]
fn unshadowed_task_still_gets_the_constant() {
    let v = vf(r#"
name := global

greet name:
    echo hello {{name}}

banner:
    echo welcome {{name}}
"#);
    assert_eq!(body_of(&v, "banner"), "echo welcome global");
}

#[test]
fn value_may_contain_spaces_and_symbols() {
    let v = vf(r#"
image := registry.example.com/app:latest
flags := -O2 -Wall

build:
    docker build -t {{image}} . {{flags}}
"#);
    assert_eq!(
        body_of(&v, "build"),
        "docker build -t registry.example.com/app:latest . -O2 -Wall"
    );
}

#[test]
fn duplicate_constant_is_an_error() {
    let err = parser::parse("x := 1\nx := 2\n").unwrap_err();
    assert!(err.message.contains("already defined"), "got: {}", err.message);
}

#[test]
fn empty_value_is_an_error() {
    let err = parser::parse("x :=\n").unwrap_err();
    assert!(err.message.contains("no value"), "got: {}", err.message);
}

#[test]
fn reference_in_value_is_an_error() {
    let err = parser::parse("greeting := hi {{name}}\n").unwrap_err();
    assert!(err.message.contains("literal"), "got: {}", err.message);
}

#[test]
fn colon_equals_in_deps_is_not_a_constant() {
    // The `:=` here is inside a dependency argument; the line must still parse
    // as a task header, not a constant definition.
    let v = vf(r#"
say word:
    echo {{word}}

demo: say "a:=b"
"#);
    let demo = v.tasks.get("demo").expect("task exists");
    assert_eq!(demo.deps[0].args, vec!["a:=b".to_string()]);
}

#[test]
fn unknown_reference_is_left_verbatim() {
    let v = vf(r#"
build:
    echo {{nonexistent}}
"#);
    assert_eq!(body_of(&v, "build"), "echo {{nonexistent}}");
}
