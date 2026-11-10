use clap::Parser;
use clap_complete::Shell;
use kitty_pptx_cli::cli::{Cli, Commands};
use kitty_pptx_cli::commands::completions::generate_completions_to;

#[test]
fn test_bash_completion_generation() {
    let mut buf = Vec::new();
    generate_completions_to(Shell::Bash, &mut buf);
    let output = String::from_utf8(buf).expect("valid utf-8");

    assert!(output.contains("kitty-pptx"));
    assert!(output.contains("--generate-completions"));
    assert!(output.contains("export"));
    assert!(output.contains("view"));
    assert!(output.contains("dump"));
}

#[test]
fn test_zsh_completion_generation() {
    let mut buf = Vec::new();
    generate_completions_to(Shell::Zsh, &mut buf);
    let output = String::from_utf8(buf).expect("valid utf-8");

    assert!(output.contains("#compdef kitty-pptx"));
    assert!(output.contains("--generate-completions"));
}

#[test]
fn test_fish_completion_generation() {
    let mut buf = Vec::new();
    generate_completions_to(Shell::Fish, &mut buf);
    let output = String::from_utf8(buf).expect("valid utf-8");

    assert!(output.contains("complete -c kitty-pptx"));
}

#[test]
fn test_elvish_and_powershell_completion_generation() {
    let mut elvish_buf = Vec::new();
    generate_completions_to(Shell::Elvish, &mut elvish_buf);
    let elvish_out = String::from_utf8(elvish_buf).expect("valid utf-8");
    assert!(elvish_out.contains("kitty-pptx"));

    let mut pwsh_buf = Vec::new();
    generate_completions_to(Shell::PowerShell, &mut pwsh_buf);
    let pwsh_out = String::from_utf8(pwsh_buf).expect("valid utf-8");
    assert!(pwsh_out.contains("kitty-pptx"));
}

#[test]
fn test_cli_parse_completions_flag() {
    let args = Cli::try_parse_from(["kitty-pptx", "--generate-completions", "bash"])
        .expect("should parse --generate-completions flag");
    assert_eq!(args.generate_completions, Some(Shell::Bash));

    let args_zsh = Cli::try_parse_from(["kitty-pptx", "--generate-completions", "zsh"])
        .expect("should parse zsh");
    assert_eq!(args_zsh.generate_completions, Some(Shell::Zsh));

    let args_fish = Cli::try_parse_from(["kitty-pptx", "--generate-completions", "fish"])
        .expect("should parse fish");
    assert_eq!(args_fish.generate_completions, Some(Shell::Fish));
}

#[test]
fn test_cli_parse_completions_subcommand() {
    let args = Cli::try_parse_from(["kitty-pptx", "completions", "zsh"])
        .expect("should parse completions subcommand");

    match args.command {
        Some(Commands::Completions { shell }) => {
            assert_eq!(shell, Shell::Zsh);
        }
        _ => panic!("Expected Completions subcommand"),
    }
}
