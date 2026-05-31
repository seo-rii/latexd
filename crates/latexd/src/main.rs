use std::{fs, io::Write};

use anyhow::{Context, Result, anyhow, bail};
use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand, ValueEnum};
use latexd::{
    EditorBridgeConfig, ServeArgs, TileRendererConfig,
    compiler::capture_internal_render_ir_from_project_root, serve,
};
use std::sync::Arc;
use tex_aux::SemanticAux;
use tex_render_gs::{GsApiRuntime, GsApiRuntimePool};

#[derive(Debug, Parser)]
#[command(name = "latexd", version, about = "Incremental LaTeX preview daemon")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve(ServeCli),
    RenderIr(RenderIrCli),
    MockCompiler(MockCompilerCli),
}

#[derive(Debug, Args)]
struct ServeCli {
    #[arg(long, default_value = ".")]
    root: String,
    #[arg(long, default_value = "127.0.0.1:4380")]
    bind: String,
    #[arg(long)]
    compiler_bin: Option<String>,
    #[arg(long = "compiler-arg")]
    compiler_args: Vec<String>,
    #[arg(long, value_enum, default_value_t = TileRendererMode::Mock)]
    tile_renderer: TileRendererMode,
    #[arg(long)]
    gs_bin: Option<String>,
    #[arg(long)]
    libgs: Option<String>,
    #[arg(long, default_value_t = 2)]
    gs_api_pool_size: usize,
    #[arg(long)]
    editor_bin: Option<String>,
    #[arg(long = "editor-arg")]
    editor_args: Vec<String>,
}

#[derive(Debug, Args)]
struct RenderIrCli {
    #[arg(long, default_value = ".")]
    root: String,
    #[arg(long)]
    input: String,
    #[arg(long)]
    output_dir: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum TileRendererMode {
    Mock,
    GsCli,
    GsApi,
}

#[derive(Debug, Args)]
struct MockCompilerCli {
    #[arg(long)]
    input: String,
    #[arg(long)]
    output: String,
    #[arg(long)]
    depfile: Option<String>,
    #[arg(long)]
    fail_if_contains: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("latexd=info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Command::Serve(command) => {
            let root = Utf8PathBuf::from(command.root);
            let tile_renderer = match command.tile_renderer {
                TileRendererMode::Mock => TileRendererConfig::Mock,
                TileRendererMode::GsCli => {
                    let program = if let Some(gs_bin) = command.gs_bin {
                        gs_bin
                    } else {
                        which::which("gs")
                            .context(
                                "failed to find `gs`; pass --gs-bin or use --tile-renderer mock",
                            )?
                            .to_string_lossy()
                            .to_string()
                    };
                    TileRendererConfig::GsCli { program }
                }
                TileRendererMode::GsApi => {
                    let runtime = Arc::new(GsApiRuntime::new(command.libgs.as_deref()).context(
                        "failed to initialize libgs runtime; pass --libgs or use --tile-renderer gs-cli/mock",
                    )?);
                    let library_path = runtime.library_path().to_string_lossy().to_string();
                    let runtime_pool = Arc::new(
                        GsApiRuntimePool::new(command.libgs.as_deref(), command.gs_api_pool_size)
                            .context("failed to initialize gs-api runtime pool")?,
                    );
                    TileRendererConfig::GsApi {
                        library_path,
                        runtime: Some(runtime),
                        runtime_pool: Some(runtime_pool),
                    }
                }
            };
            serve(ServeArgs {
                root,
                bind: command.bind,
                compiler_bin: command.compiler_bin,
                compiler_args: command.compiler_args,
                tile_renderer,
                editor_bridge: command.editor_bin.map(|program| EditorBridgeConfig {
                    program,
                    args: command.editor_args,
                }),
            })
            .await
        }
        Command::RenderIr(command) => {
            let root = Utf8PathBuf::from(command.root);
            let output_dir = Utf8PathBuf::from(command.output_dir);
            let capture = capture_internal_render_ir_from_project_root(
                &root,
                &command.input,
                &SemanticAux::default(),
            )?;
            let paths = capture.write_debug_artifacts(&output_dir)?;

            println!("wrote render IR artifacts to {output_dir}");
            println!("events: {}", paths.events);
            println!("document IR: {}", paths.document_ir);
            println!("display-list JSON: {}", paths.page_display_list);
            println!("display-list PDF: {}", paths.display_list_pdf);
            for svg in paths.display_list_svgs {
                println!("display-list SVG: {svg}");
            }
            Ok(())
        }
        Command::MockCompiler(command) => {
            let input = fs::read_to_string(&command.input)
                .with_context(|| format!("failed to read input {}", command.input))?;
            if let Some(needle) = command.fail_if_contains {
                if input.contains(&needle) {
                    bail!("mock compiler encountered forbidden token `{needle}`");
                }
            }

            let output_path = std::path::PathBuf::from(&command.output);
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create output directory {}", parent.display())
                })?;
            }

            let mut sanitized = String::new();
            for character in input.chars().take(160) {
                match character {
                    '(' | ')' | '\\' => {
                        sanitized.push('\\');
                        sanitized.push(character);
                    }
                    '\n' | '\r' => sanitized.push(' '),
                    other if other.is_control() => sanitized.push('?'),
                    other => sanitized.push(other),
                }
            }
            if sanitized.trim().is_empty() {
                sanitized.push_str("latexd preview");
            }

            let stream = format!("BT /F1 14 Tf 72 760 Td ({sanitized}) Tj ET");
            let mut objects = Vec::new();
            objects.push("1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n".to_string());
            objects.push("2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n".to_string());
            objects.push(
                "3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >> endobj\n"
                    .to_string(),
            );
            objects.push(
                "4 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n"
                    .to_string(),
            );
            objects.push(format!(
                "5 0 obj << /Length {} >> stream\n{}\nendstream\nendobj\n",
                stream.len(),
                stream
            ));

            let mut pdf = Vec::new();
            pdf.extend_from_slice(b"%PDF-1.4\n");
            let mut offsets = vec![0usize];
            for object in &objects {
                offsets.push(pdf.len());
                pdf.extend_from_slice(object.as_bytes());
            }
            let xref_offset = pdf.len();
            pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
            pdf.extend_from_slice(b"0000000000 65535 f \n");
            for offset in offsets.iter().skip(1) {
                pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            pdf.extend_from_slice(
                format!(
                    "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                    objects.len() + 1,
                    xref_offset
                )
                .as_bytes(),
            );

            let mut file = fs::File::create(&output_path)
                .with_context(|| format!("failed to create output {}", output_path.display()))?;
            file.write_all(&pdf).map_err(|error| {
                anyhow!("failed to write output {}: {error}", output_path.display())
            })?;

            if let Some(depfile) = command.depfile {
                let depfile_path = std::path::PathBuf::from(&depfile);
                if let Some(parent) = depfile_path.parent() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("failed to create depfile directory {}", parent.display())
                    })?;
                }

                let input_path = Utf8PathBuf::from(&command.input);
                let input_dir = input_path
                    .parent()
                    .map(|path| path.to_path_buf())
                    .unwrap_or_default();
                let mut dependencies = std::collections::BTreeSet::new();
                dependencies.insert(input_path.to_string());
                for pattern in ["\\input{", "\\include{", "\\includegraphics{"] {
                    let mut remaining = input.as_str();
                    while let Some(start) = remaining.find(pattern) {
                        let tail = &remaining[start + pattern.len()..];
                        let Some(end) = tail.find('}') else {
                            break;
                        };
                        let raw = tail[..end].trim();
                        if !raw.is_empty() {
                            let candidate = if (pattern == "\\input{" || pattern == "\\include{")
                                && Utf8PathBuf::from(raw).extension().is_none()
                            {
                                input_dir.join(raw).with_extension("tex")
                            } else {
                                input_dir.join(raw)
                            };
                            dependencies.insert(candidate.to_string());
                        }
                        remaining = &tail[end + 1..];
                    }
                }

                let mut depfile_contents = String::new();
                depfile_contents.push_str(&command.output);
                depfile_contents.push(':');
                for dependency in dependencies {
                    depfile_contents.push(' ');
                    depfile_contents.push_str(&dependency);
                }
                depfile_contents.push('\n');
                fs::write(&depfile_path, depfile_contents).with_context(|| {
                    format!("failed to write depfile {}", depfile_path.display())
                })?;
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_render_ir_command() {
        let cli = Cli::parse_from([
            "latexd",
            "render-ir",
            "--root",
            "/tmp/project",
            "--input",
            "main.tex",
            "--output-dir",
            "/tmp/out",
        ]);

        let Command::RenderIr(command) = cli.command else {
            panic!("expected render-ir command");
        };
        assert_eq!(command.root, "/tmp/project");
        assert_eq!(command.input, "main.tex");
        assert_eq!(command.output_dir, "/tmp/out");
    }
}
