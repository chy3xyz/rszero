use clap::{Parser, Subcommand};
use anyhow::Result;

mod cmd;

#[derive(Parser)]
#[command(name = "rszeroctl", about = "rszero CLI scaffolding tool — 对标 goctl", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new rszero project
    New {
        /// Project name
        name: String,
    },
    /// Generate API gateway code from .api definition
    Api(ApiArgs),
    /// Generate RPC service code from IDL
    Rpc(RpcArgs),
    /// Generate database model code
    Model(ModelArgs),
    /// Generate Dockerfile
    Docker(DockerArgs),
    /// Generate Swagger/OpenAPI docs
    Swagger {
        /// API definition file
        #[arg(long)]
        api: String,
        /// Output directory
        #[arg(long)]
        dir: String,
    },
    /// Generate Kubernetes deployment manifests
    Kube {
        /// Service name
        #[arg(long)]
        name: String,
        /// Output directory
        #[arg(long)]
        dir: String,
    },
    /// Generate template files
    Template {
        /// Template type (api, rpc, model)
        #[arg(long)]
        kind: String,
        /// Output directory
        #[arg(long)]
        dir: String,
    },
    /// Run HTTP load test
    Loadtest {
        /// Target URL
        #[arg(long)]
        url: String,
        /// Number of concurrent workers
        #[arg(long, default_value = "10")]
        workers: usize,
        /// Total number of requests (0 = unlimited)
        #[arg(long, default_value = "1000")]
        total: u64,
        /// Test duration in seconds (0 = until total)
        #[arg(long, default_value = "0")]
        duration: u64,
        /// HTTP method
        #[arg(long, default_value = "GET")]
        method: String,
    },
    /// Upgrade rszeroctl
    Upgrade,
    /// Show environment info
    Env,
}

#[derive(clap::Args)]
struct ApiArgs {
    #[command(subcommand)]
    command: ApiCommands,
}

#[derive(clap::Subcommand)]
enum ApiCommands {
    Go {
        #[arg(long)]
        api: String,
        #[arg(long)]
        dir: String,
    },
}

#[derive(clap::Args)]
struct RpcArgs {
    #[command(subcommand)]
    command: RpcCommands,
}

#[derive(clap::Subcommand)]
enum RpcCommands {
    Protoc {
        /// IDL file path
        idl: String,
        #[arg(long)]
        out: String,
    },
}

#[derive(clap::Args)]
struct ModelArgs {
    #[command(subcommand)]
    command: ModelCommands,
}

#[derive(clap::Subcommand)]
enum ModelCommands {
    Datasource {
        #[arg(long)]
        url: String,
        #[arg(long)]
        table: String,
        #[arg(long)]
        dir: String,
    },
}

#[derive(clap::Args)]
struct DockerArgs {
    #[arg(long)]
    go: String,
    #[arg(long)]
    out: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { name } => cmd::new::execute(&name)?,
        Commands::Api(api_args) => match api_args.command {
            ApiCommands::Go { api, dir } => cmd::api::execute(&api, &dir)?,
        },
        Commands::Rpc(rpc_args) => match rpc_args.command {
            RpcCommands::Protoc { idl, out } => cmd::rpc::execute(&idl, &out)?,
        },
        Commands::Model(model_args) => match model_args.command {
            ModelCommands::Datasource { url, table, dir } => {
                cmd::model::execute(&url, &table, &dir)?;
            }
        },
        Commands::Docker(docker_args) => {
            cmd::docker::execute(&docker_args.go, &docker_args.out)?;
        }
        Commands::Swagger { api, dir } => {
            cmd::swagger::execute(&api, &dir)?;
        }
        Commands::Kube { name, dir } => {
            cmd::kube::execute(&name, &dir)?;
        }
        Commands::Template { kind, dir } => {
            cmd::template::execute(&kind, &dir)?;
        }
        Commands::Loadtest { url, workers, total, duration, method } => {
            cmd::loadtest::execute(&url, workers, total, duration, &method).await?;
        }
        Commands::Upgrade => {
            cmd::upgrade::execute()?;
        }
        Commands::Env => {
            cmd::env::execute()?;
        }
    }

    Ok(())
}
