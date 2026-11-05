use clap::{Args, Parser, Subcommand, ValueEnum};
use std::{fs::File, path::PathBuf, process::ExitCode};
use tomlgraft_core::{Kind, Operation, Options, PatchError, Report, Scope};

#[derive(Parser)]
#[command(
    version,
    about = "Make small, transactional TOML edits; lockfiles are read-only"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[arg(long, global = true, value_enum, default_value = "text")]
    format: Format,
    #[arg(long, global = true, value_enum)]
    kind: Option<TargetKind>,
    #[arg(
        long,
        global = true,
        help = "Prepare without creating sidecars or tempfiles"
    )]
    dry_run: bool,
    #[arg(long, global = true, help = "Preview; exit 6 if changes would be made")]
    check: bool,
    #[arg(
        long,
        global = true,
        help = "Include a potentially sensitive unified diff"
    )]
    diff: bool,
    #[arg(
        long,
        global = true,
        help = "Allow additive operations to create missing PEP 723 metadata"
    )]
    create_script_block: bool,
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}
#[derive(Clone, Copy, ValueEnum)]
enum TargetKind {
    Toml,
    Pyproject,
    Script,
    Lock,
}
impl From<TargetKind> for Kind {
    fn from(k: TargetKind) -> Self {
        match k {
            TargetKind::Toml => Kind::Toml,
            TargetKind::Pyproject => Kind::Pyproject,
            TargetKind::Script => Kind::Script,
            TargetKind::Lock => Kind::Lock,
        }
    }
}
#[derive(Subcommand)]
enum Command {
    Set {
        file: PathBuf,
        path: String,
        value: String,
    },
    Rm {
        file: PathBuf,
        path: String,
    },
    Dep {
        #[command(subcommand)]
        command: Dependency,
    },
    Batch {
        file: PathBuf,
        #[arg(long, help = "Versioned JSON file, or - for stdin")]
        patch: String,
    },
    Check {
        file: PathBuf,
    },
}
#[derive(Subcommand)]
enum Dependency {
    Add {
        file: PathBuf,
        requirement: String,
        #[command(flatten)]
        scope: Selector,
    },
    Rm {
        file: PathBuf,
        selector: String,
        #[command(flatten)]
        scope: Selector,
    },
}
#[derive(Args)]
struct Selector {
    #[arg(long, conflicts_with = "optional")]
    group: Option<String>,
    #[arg(long)]
    optional: Option<String>,
}
impl Selector {
    fn scope(&self, file: &std::path::Path, options: &Options) -> Result<Scope, PatchError> {
        if let Some(name) = &self.group {
            return Ok(Scope::Group { name: name.clone() });
        }
        if let Some(name) = &self.optional {
            return Ok(Scope::Optional { name: name.clone() });
        }
        Ok(
            if tomlgraft_core::classify(file, options.kind)? == Kind::Script {
                Scope::Script
            } else {
                Scope::Project
            },
        )
    }
}

fn execute(cli: &Cli, options: &Options) -> Result<Report, PatchError> {
    let (file, ops) = match &cli.command {
        Command::Check { file } => return tomlgraft_core::check_file(file, options),
        Command::Set { file, path, value } => (
            file,
            vec![Operation::Set {
                path: tomlgraft_core::parse_path(path)?,
                value_toml: value.clone(),
            }],
        ),
        Command::Rm { file, path } => (
            file,
            vec![Operation::Remove {
                path: tomlgraft_core::parse_path(path)?,
            }],
        ),
        Command::Dep { command } => match command {
            Dependency::Add {
                file,
                requirement,
                scope,
            } => (
                file,
                vec![Operation::UpsertDependency {
                    scope: scope.scope(file, options)?,
                    requirement: requirement.clone(),
                }],
            ),
            Dependency::Rm {
                file,
                selector,
                scope,
            } => (
                file,
                vec![Operation::RemoveDependency {
                    scope: scope.scope(file, options)?,
                    selector: selector.clone(),
                }],
            ),
        },
        Command::Batch { file, patch } => {
            let bytes = if patch == "-" {
                tomlgraft_core::read_bounded(std::io::stdin().lock(), tomlgraft_core::BATCH_LIMIT)?
            } else {
                tomlgraft_core::read_bounded(
                    File::open(patch).map_err(|_| cli_error("io", "cannot open batch file", 1))?,
                    tomlgraft_core::BATCH_LIMIT,
                )?
            };
            (file, tomlgraft_core::decode_batch(&bytes)?)
        }
    };
    tomlgraft_core::patch_file(file, &ops, options)
}

fn cli_error(code: &'static str, message: &'static str, exit_code: u8) -> PatchError {
    PatchError::new(code, message, exit_code)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let options = Options {
        kind: cli.kind.map(Into::into),
        dry_run: cli.dry_run,
        check: cli.check,
        diff: cli.diff,
        create_script_block: cli.create_script_block,
    };
    let (mut report, exit) = match execute(&cli, &options) {
        Ok(report) => {
            let code = if options.check && report.changed == Some(true) {
                6
            } else {
                0
            };
            (report, code)
        }
        Err(e) => (*e.report, e.exit_code),
    };
    if report.target.is_none() {
        let file = match &cli.command {
            Command::Check { file }
            | Command::Set { file, .. }
            | Command::Rm { file, .. }
            | Command::Batch { file, .. } => file,
            Command::Dep { command } => match command {
                Dependency::Add { file, .. } | Dependency::Rm { file, .. } => file,
            },
        };
        report.target = Some(file.to_string_lossy().into_owned());
        report.target_kind = tomlgraft_core::classify(file, options.kind).ok();
    }
    if let Some(error) = &report.error {
        eprintln!("{}: {}", error.code, error.message);
    }
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let printed = match cli.format {
        Format::Json => serde_json::to_writer(&mut out, &report)
            .map_err(std::io::Error::other)
            .and_then(|_| writeln!(out)),
        Format::Text => {
            let result = writeln!(
                out,
                "{}: {} (committed={}, durability={:?})",
                report.status,
                report.target.as_deref().unwrap_or("input"),
                report.committed,
                report.durability
            );
            result.and_then(|_| {
                if let Some(diff) = &report.diff {
                    write!(out, "{diff}")
                } else if let Some(reason) = report.diff_omitted {
                    writeln!(out, "diff omitted: {reason}")
                } else {
                    Ok(())
                }
            })
        }
    };
    if printed.is_err() {
        eprintln!("output failed; reread target to determine commit state");
        return ExitCode::from(1);
    }
    ExitCode::from(exit)
}
