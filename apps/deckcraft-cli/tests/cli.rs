use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deckcraft-cli")).args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    assert!(o.status.success(), "exit {:?}\nstderr: {}", o.status, String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    assert!(!o.status.success(), "exit {:?}\nstdout: {}", o.status, String::from_utf8_lossy(&o.stdout));
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn help_explains_run() {
    for flag in ["help", "--help", "-h"] {
        let out = stdout(&cli(&[flag]));
        assert!(out.contains("--cmd COMMAND[=PARAMS]") && out.contains("COMMAND is a command id"), "{out}");
    }
    assert!(stdout(&cli(&["run", "--help"])).starts_with("run applies each --cmd in order"));
    assert!(stderr(&cli(&["frobnicate"])).starts_with("deckcraft-cli: unknown subcommand `frobnicate`\nusage:"));
}

#[test]
fn a_label_before_the_command_is_an_unknown_command() {
    assert_eq!(
        stderr(&cli(&["run", "--cmd", r#"new=file.new={"blank":true}"#])),
        "deckcraft-cli: --cmd 1 of 1: unknown command `new`; commands named like it: file.new, slide.new; nothing ran\n"
    );
}

#[test]
fn bad_json_names_the_command_and_the_text() {
    assert_eq!(
        stderr(&cli(&["run", "--cmd", "slide.new={}", "--cmd", r#"shape.insert={"preset":"rect",}"#])),
        "deckcraft-cli: --cmd 2 of 2: the parameters of `shape.insert` are not JSON (trailing comma at line 1 column 18): {\"preset\":\"rect\",}; nothing ran\n"
    );
}

#[test]
fn a_failing_command_says_which_and_what_was_kept() {
    let out = std::env::temp_dir().join(format!("dc-cli-{}.deckcraft", std::process::id()));
    let err = stderr(&cli(&[
        "run",
        "--cmd",
        r#"shape.insert={"preset":"rect","rect":[10,10,100,50]}"#,
        "--cmd",
        "slide.new",
        "--cmd",
        r#"shape.resize={"ids":[260],"w":50}"#,
        "--save",
        out.to_str().unwrap(),
    ]));
    assert_eq!(
        err,
        "deckcraft-cli: --cmd 3 of 3: invalid parameters for `shape.resize`: shape 260 is on slide index 0, not on slide index 1, \
         the one being edited; nothing was saved or exported\n"
    );
    assert!(!out.exists());
}

#[test]
fn print_shows_what_each_command_did() {
    let out = stdout(&cli(&[
        "run",
        "--print",
        "--cmd",
        r#"shape.insert={"preset":"roundRect","rect":[80,80,300,120],"text":"Revenue"}"#,
        "--cmd",
        r#"format.size={"size":28}"#,
        "--cmd",
        r##"design.background={"color":"#112233"}"##,
        "--cmd",
        r#"text.edit={"at":[0,3]}"#,
    ]));
    assert_eq!(
        out,
        "shape.insert → {\"id\":260}\n\
         format.size → changed shape 260 on slide index 0\n\
         design.background → changed the background of slide index 0\n\
         text.edit → editing the text of shape 260, paragraph 0, character 3\n"
    );
}

#[test]
fn an_unknown_parameter_is_refused() {
    assert_eq!(
        stderr(&cli(&["run", "--cmd", r#"shape.insert={"shape":"roundRect"}"#])),
        "deckcraft-cli: --cmd 1 of 1: `shape.insert` has no parameter `shape`; its parameters are preset, rect, text, adj, flip\n"
    );
}
